import os
from typing import TYPE_CHECKING

import pytest
from cyclopts import CycloptsError

from harness.daily import seeded
from harness.daily.__main__ import app
from harness.daily.report import REPORT, Report, Result

if TYPE_CHECKING:
	from pathlib import Path


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
