from typing import Annotated
lazy from pathlib import Path

from cyclopts import App, Parameter, validators

import harness.daily.fuzz
import harness.daily.seeded
from harness.daily import issue
from harness.daily.report import SAMPLE, Result, to_step_summary

app = App()


@app.command
def seeded(
	task: str,
	axis: Annotated[str, Parameter(help="the task's matrix axis that carries the seed")],
	out: Path,
	seed: Annotated[
		int | None,
		Parameter(help="reuse a seed instead of drawing one", validator=validators.Number(gte=0, lt=2**64)),
	] = None,
) -> int:
	"""Run a camas task on a fresh seed and report it."""
	return harness.daily.seeded.main(axis, task, out, seed)


@app.command
def fuzz(
	fuzzing: Annotated[harness.daily.fuzz.Fuzzing, Parameter(name="*")],
	out: Annotated[Path, Parameter(help="where the job's directory, with its report and crash inputs, goes")],
) -> int:
	"""Fuzz one target from its seeds and its grown corpus, and report it with any crash input."""
	return harness.daily.fuzz.main(fuzzing, out)


@app.command(name="fuzz-matrix")
def fuzz_matrix(
	axes: Annotated[str, Parameter(help="camas daily_fuzz --github-matrix=axes, as JSON")],
) -> int:
	"""The fuzz jobs' include matrix: each target with its job name and corpus path."""
	print(harness.daily.fuzz.Matrix.of(harness.daily.fuzz.Axes.model_validate_json(axes)).model_dump_json())
	return 0


@app.command(name="issue")
def rolling(
	matrices: Annotated[issue.Matrices, Parameter(name="*")],
	reports: Annotated[Path, Parameter(help="the downloaded artifacts, one directory per task")],
	run: Annotated[issue.Run, Parameter(name="*")],
	body: Annotated[Path, Parameter(help="where a failing verdict writes the issue body")],
	label: Annotated[
		str, Parameter(help="the rolling issue's label, which the job summary's issue link searches")
	],
) -> int:
	"""The rolling issue's verdict, as GITHUB_OUTPUT lines and a job summary."""
	verdict = issue.verdict(
		issue.jobs(
			(
				*issue.Tasks.model_validate_json(matrices.matrix).task,
				*(
					cell.job
					for cell in harness.daily.fuzz.Matrix.model_validate_json(matrices.fuzz_matrix).include
				),
			),
			reports,
		),
		run,
	)
	to_step_summary(issue.summary(verdict, label))
	match verdict:
		case issue.Clean():
			print("verdict=clean")
		case issue.Failing(title=title, body=text):
			body.write_text(text)
			print(f"verdict=failing\ntitle={title}")
	return 0


@app.command
def render(
	label: Annotated[str, Parameter(help="the label the sample's issue link searches")] = "sample",
) -> int:
	"""A dry run: a sample job summary, report job summaries and issue body.

	Raises:
		AssertionError: the sample reads as clean.
	"""
	match issue.verdict(
		(
			issue.Reported("daily_properties", SAMPLE),
			issue.Reported("daily_other", SAMPLE._replace(result=Result.PASSED, output="")),
			issue.Missing("daily_missing"),
		),
		issue.Run("https://github.com/JPHutchins/cbor-static", "1", "0123456789abcdef", "main"),
	):
		case issue.Failing() as failing:
			print(
				f"# Job summary\n\n{SAMPLE.markdown()}\n"
				f"# Report job summary, failing\n\n{issue.summary(failing, label)}\n"
				f"# Report job summary, clean\n\n{issue.summary(issue.Clean(), label)}\n"
				f"# Issue: {failing.title}\n\n{failing.body}"
			)
		case issue.Clean():
			raise AssertionError("a sample with a failing job reads as clean")
	return 0


if __name__ == "__main__":
	app()
