import os
from typing import TYPE_CHECKING

import pytest
from cyclopts import CycloptsError

from harness.daily import fuzz, issue, seeded
from harness.daily.__main__ import app
from harness.daily.report import REPORT, SAMPLE, Report, Result

if TYPE_CHECKING:
	from pathlib import Path

FUZZED = fuzz.report(fuzz.Fuzzing("oracle", 900, 4), 0, "", (), 0)


def seeded_with(seed: int) -> list[str]:
	return ["seeded", "properties", "--axis", "SEED", "--out", "out", "--seed", str(seed)]


@pytest.mark.parametrize("seed", [-1, 2**64])
def test_a_seed_outside_proptests_u64_is_rejected(seed: int) -> None:
	with pytest.raises(CycloptsError):
		app.parse_args(seeded_with(seed), exit_on_error=False, print_error=False)


@pytest.mark.parametrize("seed", [0, 2**64 - 1])
def test_a_seed_inside_proptests_u64_parses(seed: int) -> None:
	assert app.parse_args(seeded_with(seed), exit_on_error=False)[1].arguments["seed"] == seed


@pytest.mark.parametrize(("code", "result"), [(0, Result.PASSED), (3, Result.FAILED)])
def test_a_seeded_run_reports_to_the_summary_and_passes_its_exit_code_through(
	tmp_path: Path, monkeypatch: pytest.MonkeyPatch, code: int, result: Result
) -> None:
	(tmp_path / "bin").mkdir()
	(tmp_path / "bin" / "camas").write_text(f'#!/bin/sh\necho "camas $*"\nexit {code}\n')
	(tmp_path / "bin" / "camas").chmod(0o755)
	monkeypatch.setenv("PATH", f"{tmp_path / 'bin'}{os.pathsep}{os.environ['PATH']}")
	monkeypatch.setenv("GITHUB_STEP_SUMMARY", str(tmp_path / "summary.md"))

	assert seeded.main("SEED", "properties", tmp_path / "out", 7) == code
	written = Report.loads((tmp_path / "out" / "properties" / REPORT).read_text())
	assert (written.result, written.parameters) == (result, (("SEED", "7"),))
	assert (tmp_path / "summary.md").read_text() == written.markdown()


@pytest.mark.parametrize("result", [Result.PASSED, Result.FAILED])
def test_the_issue_command_writes_the_verdicts_summary_without_the_task_reports(
	tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str], result: Result
) -> None:
	reported = SAMPLE._replace(result=result)
	(tmp_path / "reports" / "daily_properties" / "properties").mkdir(parents=True)
	(tmp_path / "reports" / "daily_properties" / "properties" / REPORT).write_text(reported.dumps())
	(tmp_path / "reports" / "daily_fuzz-oracle" / "daily_fuzz-oracle").mkdir(parents=True)
	(tmp_path / "reports" / "daily_fuzz-oracle" / "daily_fuzz-oracle" / REPORT).write_text(FUZZED.dumps())
	monkeypatch.setenv("GITHUB_STEP_SUMMARY", str(tmp_path / "summary.md"))
	run = issue.Run("https://github.com/o/r", "42", "0123456789abcdef", "main")

	with pytest.raises(SystemExit) as exited:
		app(
			[
				"issue",
				"--matrix",
				'{"task": ["daily_properties"]}',
				"--fuzz-matrix",
				fuzz.Matrix.of(fuzz.Axes(TARGET=("oracle",))).model_dump_json(),
				"--reports",
				str(tmp_path / "reports"),
				"--repository-url",
				run.repository_url,
				"--run-id",
				run.run_id,
				"--sha",
				run.sha,
				"--ref",
				run.ref,
				"--body",
				str(tmp_path / "issue.md"),
				"--label",
				"rolling",
			]
		)

	verdict = issue.verdict(
		(issue.Reported("daily_properties", reported), issue.Reported("daily_fuzz-oracle", FUZZED)), run
	)
	assert exited.value.code == 0
	assert (tmp_path / "summary.md").read_text() == issue.summary(verdict, "rolling")
	assert "<details>" not in (tmp_path / "summary.md").read_text()
	match verdict:
		case issue.Clean():
			assert (capsys.readouterr().out, (tmp_path / "issue.md").exists()) == ("verdict=clean\n", False)
			assert (tmp_path / "summary.md").read_text() == "### ✅ Daily run clean\n"
		case issue.Failing(title=title, body=body):
			assert capsys.readouterr().out == f"verdict=failing\ntitle={title}\n"
			assert (tmp_path / "issue.md").read_text() == body
			assert "label%3A%22rolling%22" in (tmp_path / "summary.md").read_text()
			assert "| daily_properties | ❌ failed |" in (tmp_path / "summary.md").read_text()


@pytest.mark.parametrize(
	("code", "finding", "result"),
	[(0, None, Result.PASSED), (1, "crash-0a1b", Result.FAILED), (0, "oom-0a1b", Result.FAILED)],
)
def test_a_fuzz_run_reports_its_findings_and_fails_on_any(
	tmp_path: Path,
	monkeypatch: pytest.MonkeyPatch,
	code: int,
	finding: str | None,
	result: Result,
) -> None:
	(tmp_path / "out" / "daily_fuzz-oracle").mkdir(parents=True)
	(tmp_path / "out" / "daily_fuzz-oracle" / "crash-stale").write_bytes(b"\x00")
	(tmp_path / "bin").mkdir()
	(tmp_path / "bin" / "camas").write_text(
		"#!/bin/sh\n"
		'echo "camas $*"\n'
		+ (
			f'for arg; do case $arg in --ARTIFACTS=*) printf "\\202\\001" > "${{arg#--ARTIFACTS=}}/{finding}";; esac; done\n'
			if finding
			else ""
		)
		+ f"exit {code}\n"
	)
	(tmp_path / "bin" / "camas").chmod(0o755)
	monkeypatch.setenv("PATH", f"{tmp_path / 'bin'}{os.pathsep}{os.environ['PATH']}")
	monkeypatch.setenv("GITHUB_STEP_SUMMARY", str(tmp_path / "summary.md"))

	assert fuzz.main(fuzz.Fuzzing("oracle", 60, 1), tmp_path / "out") == int(result is Result.FAILED)
	written = Report.loads((tmp_path / "out" / "daily_fuzz-oracle" / REPORT).read_text())
	assert written.result == result
	assert "crash-stale" not in written.output
	assert (f"{finding}, 2 bytes: 8201" in written.output) == (finding is not None)
	assert (tmp_path / "summary.md").read_text() == written.markdown()


def test_a_fuzz_job_without_a_report_reads_as_missing(tmp_path: Path) -> None:
	assert issue.jobs((fuzz.job("cose"),), tmp_path) == (issue.Missing("daily_fuzz-cose"),)
