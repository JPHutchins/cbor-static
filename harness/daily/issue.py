"""The rolling daily-run issue: one body naming every job a run discovered, or nothing on a clean run."""

from typing import TYPE_CHECKING, Final, NamedTuple
from urllib.parse import quote_plus

from harness.daily.report import REPORT, Report, Result, cell, code

if TYPE_CHECKING:
	from pathlib import Path

BODY_LIMIT: Final = 60000


class Reported(NamedTuple):
	task: str
	report: Report


class Missing(NamedTuple):
	task: str


class Unreadable(NamedTuple):
	task: str
	reason: str


type Job = Reported | Missing | Unreadable


class Clean(NamedTuple):
	pass


class Failing(NamedTuple):
	jobs: tuple[Job, ...]
	run: Run

	@property
	def title(self) -> str:
		return (
			f"Daily run failed on {self.run.ref}: "
			f"{', '.join(job.task for job in self.jobs if not passed(job)) or 'no jobs ran'} ({self.run.sha[:7]})"
		)

	@property
	def body(self) -> str:
		return fitted(self.jobs, self.run)


type Verdict = Clean | Failing


class Run(NamedTuple):
	repository_url: str
	run_id: str
	sha: str
	ref: str

	@property
	def url(self) -> str:
		return f"{self.repository_url}/actions/runs/{self.run_id}"


def jobs(tasks: tuple[str, ...], reports: Path) -> tuple[Job, ...]:
	"""
	>>> import tempfile
	>>> from pathlib import Path
	>>> from harness.daily.report import SAMPLE
	>>> with tempfile.TemporaryDirectory() as tmp:
	...     for task, text in (("daily_properties", SAMPLE.dumps()), ("daily_cut", '{"title"')):
	...         _ = (Path(tmp) / task / "properties").mkdir(parents=True)
	...         _ = (Path(tmp) / task / "properties" / REPORT).write_text(text)
	...     [type(job).__name__ for job in jobs(("daily_properties", "daily_cut", "daily_fuzz"), Path(tmp))]
	['Reported', 'Unreadable', 'Missing']
	>>> with tempfile.TemporaryDirectory() as tmp:
	...     _ = (Path(tmp) / "daily_properties" / "properties").mkdir(parents=True)
	...     _ = (Path(tmp) / "daily_properties" / "properties" / REPORT).write_text(SAMPLE.dumps())
	...     jobs(("daily_properties",), Path(tmp))[0].task
	'daily_properties'
	>>> with tempfile.TemporaryDirectory() as tmp:
	...     _ = (Path(tmp) / "properties").mkdir()
	...     _ = (Path(tmp) / "properties" / REPORT).write_text(SAMPLE.dumps())
	...     jobs(("daily_properties",), Path(tmp))
	(Missing(task='daily_properties'),)
	"""
	return tuple(job(task, tuple((reports / task).glob(f"**/{REPORT}"))) for task in tasks)


def job(task: str, found: tuple[Path, ...]) -> Job:
	"""
	>>> from pathlib import Path
	>>> job("daily_x", (Path("a"), Path("b")))
	Unreadable(task='daily_x', reason='2 reports')
	"""
	match found:
		case (report,):
			try:
				return Reported(task, Report.loads(report.read_text()))
			except ValueError as error:
				return Unreadable(task, str(error)[:200])
		case ():
			return Missing(task)
		case _:
			return Unreadable(task, f"{len(found)} reports")


def verdict(jobs: tuple[Job, ...], run: Run) -> Verdict:
	"""
	>>> from harness.daily.report import SAMPLE
	>>> RUN = Run("https://github.com/o/r", "42", "0123456789abcdef", "main")
	>>> PASSED = Reported("daily_properties", SAMPLE._replace(result=Result.PASSED, output=""))
	>>> verdict((PASSED,), RUN)
	Clean()
	>>> verdict((), RUN).title
	'Daily run failed on main: no jobs ran (0123456)'
	>>> failing = verdict(
	...     (PASSED, Reported("daily_x", SAMPLE), Missing("daily_y"), Unreadable("daily_z", "2 reports")), RUN
	... )
	>>> failing.title
	'Daily run failed on main: daily_x, daily_y, daily_z (0123456)'
	>>> print(failing.body)
	Daily run [42](https://github.com/o/r/actions/runs/42) on `main` at [`0123456`](https://github.com/o/r/commit/0123456789abcdef) failed. The next clean run on the default branch closes this issue.
	<BLANKLINE>
	Fix it with a named regression test built from the failing input below, not from the seed: a seed reproduces only while the generators draw in the same order.
	<BLANKLINE>
	| job | result | parameters | reproduce |
	|---|---|---|---|
	| daily_properties | ✅ passed | SEED=`7` | `nix develop -c camas properties --SEED=7` |
	| daily_x | ❌ failed | SEED=`7` | `nix develop -c camas properties --SEED=7` |
	| daily_y | ⚠️ no report | | the job's log, in the run |
	| daily_z | ⚠️ unreadable report: 2 reports | | the job's log, in the run |
	<BLANKLINE>
	Artifacts: [the run's artifacts](https://github.com/o/r/actions/runs/42#artifacts)
	<BLANKLINE>
	<details><summary>daily_x</summary>
	<BLANKLINE>
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
	</details>
	<BLANKLINE>
	"""
	return Clean() if jobs and all(map(passed, jobs)) else Failing(jobs, run)


def summary(verdict: Verdict, label: str) -> str:
	r"""
	>>> from harness.daily.report import SAMPLE
	>>> summary(Clean(), "rolling")
	'### ✅ Daily run clean\n'
	>>> print(summary(Failing((Reported("daily_x", SAMPLE),), Run("https://github.com/o/r", "42", "0123456", "main")), "rolling"))
	### ❌ Daily run failed on main: daily_x (0123456)
	<BLANKLINE>
	| job | result | parameters | reproduce |
	|---|---|---|---|
	| daily_x | ❌ failed | SEED=`7` | `nix develop -c camas properties --SEED=7` |
	<BLANKLINE>
	Artifacts: [the run's artifacts](https://github.com/o/r/actions/runs/42#artifacts)
	<BLANKLINE>
	Rolling issue: [the `rolling` issues](https://github.com/o/r/issues?q=is%3Aissue+label%3A%22rolling%22)
	<BLANKLINE>
	"""
	match verdict:
		case Clean():
			return "### ✅ Daily run clean\n"
		case Failing(title=title, jobs=jobs, run=run):
			return (
				f"### ❌ {title}\n\n{results(jobs, run)}\n"
				f"Rolling issue: [the {code(label)} issues]"
				f"({run.repository_url}/issues?q={quote_plus(f'is:issue label:"{label}"')})\n"
			)


def passed(job: Job) -> bool:
	match job:
		case Reported(report=report):
			return report.result is Result.PASSED
		case Missing() | Unreadable():
			return False


def fitted(jobs: tuple[Job, ...], run: Run, output_limit: int | None = None) -> str:
	r"""
	>>> from harness.daily.report import SAMPLE
	>>> huge = Reported("daily_x", SAMPLE._replace(output=("`" * 9000 + "x\n") * 20))
	>>> len(fitted((huge, huge, huge), Run("r", "1", "s", "main"))) <= BODY_LIMIT
	True
	"""
	match body(jobs, run, output_limit):
		case whole if len(whole) <= BODY_LIMIT:
			return whole
		case whole if output_limit == 0:
			return whole[:BODY_LIMIT]
		case _:
			return fitted(jobs, run, BODY_LIMIT if output_limit is None else output_limit // 2)


def body(jobs: tuple[Job, ...], run: Run, output_limit: int | None) -> str:
	return (
		f"Daily run [{run.run_id}]({run.url}) on {code(run.ref)} at [`{run.sha[:7]}`]"
		f"({run.repository_url}/commit/{run.sha}) failed. The next clean run on the default branch closes this issue.\n\n"
		"Fix it with a named regression test built from the failing input below, not from the seed: "
		"a seed reproduces only while the generators draw in the same order.\n\n"
		+ results(jobs, run)
		+ "".join(
			f"\n<details><summary>{job.task}</summary>\n\n{job.report.markdown(output_limit)}\n</details>\n"
			for job in jobs
			if isinstance(job, Reported) and not passed(job)
		)
	)


def results(jobs: tuple[Job, ...], run: Run) -> str:
	return (
		"| job | result | parameters | reproduce |\n|---|---|---|---|\n"
		+ "\n".join(map(row, jobs))
		+ f"\n\nArtifacts: [the run's artifacts]({run.url}#artifacts)\n"
	)


def row(job: Job) -> str:
	match job:
		case Reported(task=task, report=report):
			return (
				f"| {task} | {report.result.mark} {report.result} | "
				f"{cell(', '.join(f'{name}={code(value)}' for name, value in report.parameters))} | "
				f"{cell(code(report.reproduce))} |"
			)
		case Missing(task=task):
			return f"| {task} | ⚠️ no report | | the job's log, in the run |"
		case Unreadable(task=task, reason=reason):
			return f"| {task} | ⚠️ unreadable report: {cell(reason)} | | the job's log, in the run |"
