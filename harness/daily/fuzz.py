"""A fuzz target fuzzed for a while from its seeds and the corpus earlier runs grew, reported with
any input that crashed it.
"""

import os
import re
from dataclasses import dataclass
from typing import Annotated, Final
lazy from pathlib import Path

from cyclopts import Parameter, validators
from pydantic import BaseModel, ConfigDict

from harness.daily.report import REPORT, SUMMARY, Report, Result, to_step_summary
from harness.daily.seeded import LOG, TAIL_LINES, run
from harness.workspace import FUZZ_CORPUS, ROOT

JOB: Final = "daily_fuzz"
FINDINGS: Final = ("crash-", "leak-", "timeout-", "oom-", "slow-unit-")
SHOWN_BYTES: Final = 512
PROGRESS: Final = re.compile(r"(#\d+: cov: \d+.*)$", re.MULTILINE)


@dataclass(frozen=True)
class Fuzzing:
	target: Annotated[str, Parameter(help="one of fuzz/Cargo.toml's [[bin]] names")]
	seconds: Annotated[int, Parameter(help="how long libFuzzer fuzzes", validator=validators.Number(gt=0))]
	fork: Annotated[
		int, Parameter(help="libFuzzer's -fork: worker processes", validator=validators.Number(gt=0))
	]

	@property
	def invocation(self) -> str:
		"""
		>>> Fuzzing("oracle", 900, 4).invocation
		'camas fuzz.fuzz --TARGET=oracle --SECONDS=900 --FORK=4'
		"""
		return f"camas fuzz.fuzz --TARGET={self.target} --SECONDS={self.seconds} --FORK={self.fork}"


class Axes(BaseModel):
	"""camas's `--github-matrix=axes` for `daily_fuzz`."""

	model_config = ConfigDict(extra="forbid", frozen=True)
	TARGET: tuple[str, ...]


class Cell(BaseModel):
	"""One fuzz job of the daily workflow: its target, the name it reports and uploads under, and the
	corpus it carries in the Actions cache.
	"""

	model_config = ConfigDict(extra="forbid", frozen=True)
	TARGET: str
	job: str
	corpus: str


class Matrix(BaseModel):
	model_config = ConfigDict(extra="forbid", frozen=True)
	include: tuple[Cell, ...]

	@classmethod
	def of(cls, axes: Axes) -> Matrix:
		"""
		>>> Matrix.of(Axes(TARGET=("oracle",))).include[0]
		Cell(TARGET='oracle', job='daily_fuzz-oracle', corpus='target/fuzz/corpus/oracle')
		"""
		return cls(
			include=tuple(
				Cell(
					TARGET=target,
					job=job(target),
					corpus=str((Path(FUZZ_CORPUS) / target).relative_to(ROOT)),
				)
				for target in axes.TARGET
			)
		)


def job(target: str) -> str:
	"""
	>>> job("oracle")
	'daily_fuzz-oracle'
	"""
	return f"{JOB}-{target}"


def main(fuzzing: Fuzzing, out: Path) -> int:
	directory = out.resolve() / job(fuzzing.target)
	directory.mkdir(parents=True, exist_ok=True)
	for stale in found(directory):
		stale.unlink()
	returncode = run(
		(*fuzzing.invocation.split(), f"--ARTIFACTS={directory}", "--effects=(Status(output_mode='all'),)"),
		directory / LOG,
		os.environ,
	)
	written = report(
		fuzzing,
		returncode,
		(directory / LOG).read_text(errors="replace"),
		found(directory),
		sum(1 for _ in (Path(FUZZ_CORPUS) / fuzzing.target).glob("*")),
	)
	(directory / REPORT).write_text(written.dumps())
	(directory / SUMMARY).write_text(written.markdown())
	to_step_summary(written.markdown())
	return returncode or int(written.result is Result.FAILED)


def found(directory: Path) -> tuple[Path, ...]:
	"""The inputs libFuzzer wrote to `directory`: crashes, leaks, timeouts, OOMs and slow units."""
	return tuple(sorted(path for path in directory.iterdir() if path.name.startswith(FINDINGS)))


def report(fuzzing: Fuzzing, returncode: int, log: str, findings: tuple[Path, ...], corpus: int) -> Report:
	r"""
	>>> import tempfile
	>>> from pathlib import Path
	>>> log = (
	...     "#2 INITED cov: 5 ft: 5 corp: 1/1b\n[12:00] · [cargo fuzz run] #9: cov: 7 ft: 9 corp: 3 exec/s: 4 time: 1s\n"
	...     "[12:01] · [cargo fuzz cmin] #212\tDONE   cov: 8 ft: 9 exec/s: 0 rss: 1Mb\n"
	... )
	>>> clean = report(Fuzzing("oracle", 900, 4), 0, log, (), 12)
	>>> clean.result, clean.parameters[-1], clean.output
	(<Result.PASSED: 'passed'>, ('progress', '#9: cov: 7 ft: 9 corp: 3 exec/s: 4 time: 1s'), '')
	>>> clean.reproduce
	'nix develop -c camas fuzz.fuzz --TARGET=oracle --SECONDS=900 --FORK=4'
	>>> with tempfile.TemporaryDirectory() as tmp:
	...     crash = Path(tmp) / "crash-0a1b"
	...     _ = crash.write_bytes(b"\x82\x01")
	...     crashed = report(Fuzzing("oracle", 900, 4), 1, log + "panicked\n", (crash,), 12)
	>>> crashed.result, crashed.reproduce
	(<Result.FAILED: 'failed'>, 'nix develop -c camas fuzz.reproduce --TARGET=oracle --INPUT="$PWD/crash-0a1b"')
	>>> crashed.output.splitlines()[-1]
	'crash-0a1b, 2 bytes: 8201'
	>>> report(Fuzzing("oracle", 900, 4), 1, "error: could not compile", (), 0).parameters[-1]
	('progress', 'none')
	"""
	return Report(
		title=f"`cargo fuzz` on {fuzzing.target} for {fuzzing.seconds} s",
		result=Result.PASSED if returncode == 0 and not findings else Result.FAILED,
		parameters=(
			("TARGET", fuzzing.target),
			("seconds", str(fuzzing.seconds)),
			("fork", str(fuzzing.fork)),
			("corpus", str(corpus)),
			("progress", (PROGRESS.findall(log) or ["none"])[-1]),
		),
		ran=fuzzing.invocation,
		reproduce=(
			f'nix develop -c camas fuzz.reproduce --TARGET={fuzzing.target} --INPUT="$PWD/{findings[0].name}"'
			if findings
			else f"nix develop -c {fuzzing.invocation}"
		),
		output=""
		if returncode == 0 and not findings
		else "".join(log.splitlines(keepends=True)[-TAIL_LINES:])
		+ "".join(shown(finding) for finding in findings),
	)


def shown(finding: Path) -> str:
	"""A finding's name, length and leading bytes in hex, after the log tail so trimming keeps it: the
	input a named regression test is built from.
	"""
	contents = finding.read_bytes()
	return (
		f"\n{finding.name}, {len(contents)} bytes: {contents[:SHOWN_BYTES].hex()}"
		f"{' …' if len(contents) > SHOWN_BYTES else ''}\n"
	)
