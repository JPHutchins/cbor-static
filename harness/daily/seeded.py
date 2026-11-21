"""A camas task run on a fresh seed, which the report names so a failure reproduces locally."""

import secrets
import subprocess
from typing import TYPE_CHECKING, Final

from harness.daily.report import REPORT, SUMMARY, Report, Result, to_step_summary

if TYPE_CHECKING:
	from pathlib import Path

TAIL_LINES: Final = 120
LOG: Final = "output.log"


def main(axis: str, task: str, out: Path, seed: int | None) -> int:
	drawn = secrets.randbits(64) if seed is None else seed
	print(f"{axis}={drawn}", flush=True)
	directory = out / task
	directory.mkdir(parents=True, exist_ok=True)
	returncode = run(("camas", task, f"--{axis}={drawn}", "--effects=(Status(),)"), directory / LOG)
	written = report(axis, task, drawn, returncode, (directory / LOG).read_text(errors="replace"))
	(directory / REPORT).write_text(written.dumps())
	(directory / SUMMARY).write_text(written.markdown())
	to_step_summary(written.markdown())
	return returncode


def run(argv: tuple[str, ...], log: Path) -> int:
	r"""
	>>> import sys
	>>> import tempfile
	>>> from pathlib import Path
	>>> with tempfile.TemporaryDirectory() as tmp:
	...     log = Path(tmp) / LOG
	...     code = run((sys.executable, "-c", "print('drawn'); raise SystemExit(3)"), log)
	...     code, log.read_text()
	drawn
	(3, 'drawn\n')
	"""
	with (
		log.open("w", buffering=1) as written,
		subprocess.Popen(
			argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace"
		) as process,
	):
		for line in process.stdout or ():
			print(line, end="", flush=True)
			written.write(line)
	return process.returncode


def report(axis: str, task: str, seed: int, returncode: int, log: str) -> Report:
	r"""
	>>> passed = report("SEED", "properties", 7, 0, "ok\n")
	>>> passed.result, passed.output, passed.reproduce
	(<Result.PASSED: 'passed'>, '', 'nix develop -c camas properties --SEED=7')
	>>> failed = report("SEED", "properties", 7, 101, "".join(f"{n}\n" for n in range(TAIL_LINES + 5)))
	>>> failed.result, failed.output.splitlines()[0], len(failed.output.splitlines()) == TAIL_LINES
	(<Result.FAILED: 'failed'>, '5', True)
	"""
	invocation = f"camas {task} --{axis}={seed}"
	return Report(
		title=f"`camas {task}` on a fresh seed",
		result=Result.PASSED if returncode == 0 else Result.FAILED,
		parameters=((axis, str(seed)),),
		ran=invocation,
		reproduce=f"nix develop -c {invocation}",
		output="" if returncode == 0 else "".join(log.splitlines(keepends=True)[-TAIL_LINES:]),
	)
