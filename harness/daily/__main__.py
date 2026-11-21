import json
from typing import Annotated
lazy from pathlib import Path

from cyclopts import App, Parameter, validators

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


@app.command(name="issue")
def rolling(
	matrix: Annotated[str, Parameter(help='the discover job\'s {"task": [...]} matrix, as JSON')],
	reports: Annotated[Path, Parameter(help="the downloaded artifacts, one directory per task")],
	run: Annotated[issue.Run, Parameter(name="*")],
	body: Annotated[Path, Parameter(help="where a failing verdict writes the issue body")],
) -> int:
	"""The rolling issue's verdict, as GITHUB_OUTPUT lines and a job summary."""
	verdict = issue.verdict(issue.jobs(tuple(json.loads(matrix)["task"]), reports), run)
	to_step_summary(issue.summary(verdict))
	match verdict:
		case issue.Clean():
			print("verdict=clean")
		case issue.Failing(title=title, body=text):
			body.write_text(text)
			print(f"verdict=failing\ntitle={title}")
	return 0


@app.command
def render() -> int:
	"""A dry run: a sample job summary and issue body.

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
		case issue.Failing(title=title, body=body):
			print(f"# Job summary\n\n{SAMPLE.markdown()}\n# Issue: {title}\n\n{body}")
		case issue.Clean():
			raise AssertionError("a sample with a failing job reads as clean")
	return 0


if __name__ == "__main__":
	app()
