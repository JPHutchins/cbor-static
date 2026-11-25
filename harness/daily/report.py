"""A daily job's report: what ran, with which parameters, its result, and how to reproduce it."""

import json
import os
import re
from enum import StrEnum
from pathlib import Path
from typing import Final, NamedTuple

from pydantic import BaseModel, ConfigDict

REPORT: Final = "report.json"
SUMMARY: Final = "report.md"
ANSI: Final = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


class Tally(BaseModel):
	"""How many items reached each case a run counts, which a task writes where `DAILY_TALLY` says."""

	model_config = ConfigDict(extra="forbid", frozen=True)
	rows: dict[str, int]


class Result(StrEnum):
	PASSED = "passed"
	FAILED = "failed"

	@property
	def mark(self) -> str:
		match self:
			case Result.PASSED:
				return "✅"
			case Result.FAILED:
				return "❌"


class Report(NamedTuple):
	"""
	>>> print(SAMPLE.markdown())
	### ❌ `camas properties` on a fresh seed: failed
	<BLANKLINE>
	| | |
	|---|---|
	| SEED | `7` |
	| ran | `camas properties --SEED=7` |
	| reproduce | `nix develop -c camas properties --SEED=7` |
	<BLANKLINE>
	<details><summary>Output tail</summary>
	<BLANKLINE>
	````text
	minimal failing input: ``` docs
	````
	<BLANKLINE>
	</details>
	<BLANKLINE>
	>>> Report.loads(SAMPLE.dumps()) == SAMPLE
	True
	>>> counted = SAMPLE._replace(counts=Tally(rows={"refused": 2}))
	>>> Report.loads(counted.dumps()) == counted
	True
	>>> print(SAMPLE._replace(result=Result.PASSED, output="").markdown().splitlines()[0])
	### ✅ `camas properties` on a fresh seed: passed
	"""

	title: str
	result: Result
	parameters: tuple[tuple[str, str], ...]
	ran: str
	reproduce: str
	output: str
	counts: Tally = Tally(rows={})

	def markdown(self, output_limit: int | None = None) -> str:
		return (
			f"### {self.result.mark} {self.title}: {self.result}\n\n{self.table()}\n"
			f"{self.counted()}{self.tail(output_limit)}"
		)

	def table(self) -> str:
		return "\n".join(
			(
				"| | |",
				"|---|---|",
				*(
					f"| {cell(name)} | {cell(code(value))} |"
					for name, value in (*self.parameters, ("ran", self.ran), ("reproduce", self.reproduce))
				),
			)
		)

	def counted(self) -> str:
		"""
		How many items reached each case the run counts, and a warning for each case none reached.

		>>> print(SAMPLE._replace(counts=Tally(rows={"collapsed": 3, "refused": 0})).counted())
		<BLANKLINE>
		| counted | items |
		|---|---|
		| collapsed | 3 |
		| refused | 0 |
		<BLANKLINE>
		> [!WARNING]
		> No item reached `refused`: the generator never produced that case.
		<BLANKLINE>
		>>> SAMPLE.counted()
		''
		"""
		return (
			""
			if not self.counts.rows
			else "\n".join(
				(
					"",
					"| counted | items |",
					"|---|---|",
					*(f"| {cell(name)} | {count} |" for name, count in self.counts.rows.items()),
					"",
					*(
						f"> [!WARNING]\n> No item reached {code(name)}: the generator never produced that case.\n"
						for name, count in self.counts.rows.items()
						if count == 0
					),
				)
			)
		)

	def tail(self, output_limit: int | None) -> str:
		return (
			""
			if not self.output
			else "\n<details><summary>Output tail</summary>\n\n"
			+ fenced(
				self.output
				if output_limit is None
				else self.output[max(0, len(self.output) - output_limit) :]
			)
			+ "\n\n</details>\n"
		)

	def dumps(self) -> str:
		return json.dumps({**self._asdict(), "counts": self.counts.model_dump()}, indent="\t")

	@classmethod
	def loads(cls, text: str) -> Report:
		"""
		>>> Report.loads('{"title": "t"}')
		Traceback (most recent call last):
		...
		ValueError: not a daily report: {'title': 't'}

		Raises:
			ValueError: the text is not a report.
		"""
		match json.loads(text):
			case {
				"title": str(title),
				"result": str(result),
				"parameters": list(parameters),
				"ran": str(ran),
				"reproduce": str(reproduce),
				"output": str(output),
				**rest,
			}:
				return cls(
					title,
					Result(result),
					tuple(map(parameter, parameters)),
					ran,
					reproduce,
					output,
					Tally.model_validate(rest.get("counts", {"rows": {}})),
				)
			case other:
				raise ValueError(f"not a daily report: {other!r}")


def to_step_summary(markdown: str) -> None:
	if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
		with Path(summary).open("a") as step_summary:
			step_summary.write(markdown)


def parameter(pair: object) -> tuple[str, str]:
	match pair:
		case [str(name), str(value)]:
			return name, value
		case other:
			raise ValueError(f"not a report parameter: {other!r}")


def code(text: str) -> str:
	"""
	>>> code("a ` b")
	'`` a ` b ``'
	"""
	ticks = "`" * (max(map(len, re.findall("`+", text)), default=0) + 1)
	return f"{ticks} {text} {ticks}" if "`" in text else f"{ticks}{text}{ticks}"


def cell(text: str) -> str:
	r"""
	>>> cell("a | b")
	'a \\| b'
	"""
	return text.replace("|", r"\|")


def fenced(text: str) -> str:
	r"""
	>>> print(fenced("``\x1b[0m`"))
	````text
	```
	````
	"""
	return fence(ANSI.sub("", text).rstrip())


def fence(text: str) -> str:
	ticks = "`" * max(3, max(map(len, re.findall("`+", text)), default=0) + 1)
	return f"{ticks}text\n{text}\n{ticks}"


SAMPLE: Final = Report(
	title="`camas properties` on a fresh seed",
	result=Result.FAILED,
	parameters=(("SEED", "7"),),
	ran="camas properties --SEED=7",
	reproduce="nix develop -c camas properties --SEED=7",
	output="\x1b[31mminimal failing input: ```\x1b[0m \x1b]8;;https://doc.rust-lang.org\x1b\\docs\x1b]8;;\x1b\\\n",
)
