"""Libtest filters that split a test run into one slice per module and one for the rest, and a check
that the slices list exactly the run's tests.
"""

import shlex
import subprocess
from typing import Annotated

from cyclopts import App, Parameter


def slices(modules: tuple[str, ...]) -> tuple[str, ...]:
	"""
	>>> slices(("messages", "keys"))
	('messages::', 'keys::', '--skip messages:: --skip keys::')
	"""
	return (*(f"{module}::" for module in modules), " ".join(f"--skip {module}::" for module in modules))


def tests_in(listing: str) -> int:
	r"""
	>>> tests_in("a::one: test\na::two: test\nsrc/lib.rs - x (line 3): test\nb: benchmark\n")
	3
	"""
	return sum(line.endswith(": test") for line in listing.splitlines())


def listed(cargo: str, test_filter: str) -> int:
	return tests_in(
		subprocess.run(
			[*shlex.split(cargo), "--", "--list", "--format", "terse", *shlex.split(test_filter)],
			capture_output=True,
			text=True,
			check=True,
		).stdout
	)


app = App(help=__doc__)


@app.default
def check(
	modules: tuple[str, ...],
	cargo: Annotated[str, Parameter(help="The `cargo test` command whose tests the slices split.")],
) -> None:
	"""
	Raises:
		SystemExit: the slices list more or fewer tests than the run.
	"""
	counts = tuple((test_filter, listed(cargo, test_filter)) for test_filter in slices(modules))
	if sum(count for _, count in counts) != (total := listed(cargo, "")):
		raise SystemExit(f"the slices list {counts}; the run has {total} tests")


if __name__ == "__main__":
	app()
