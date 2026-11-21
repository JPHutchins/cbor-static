"""Build and measure one backend over N message types, both profiles; append rows to the results CSV."""

import csv
from typing import TYPE_CHECKING, Annotated, Final

from cyclopts import App, Parameter

from harness.backends import BACKENDS
from harness.measure import COUNTS, GATE_TARGET, HARNESS, PROFILES, Measurement, measure
from harness.model import SHAPES

if TYPE_CHECKING:
	from pathlib import Path

RESULTS: Final = HARNESS / "results"


def at_least_one_count(_: type, counts: tuple[int, ...]) -> None:
	"""
	>>> at_least_one_count(tuple, ())
	Traceback (most recent call last):
	...
	ValueError: a sweep measures at least one count

	Raises:
		ValueError: no count to measure.
	"""
	if not counts:
		raise ValueError("a sweep measures at least one count")


app = App(help=__doc__)


@app.default
def main(
	backend: Annotated[str, Parameter(choices=tuple(BACKENDS))],
	shape: Annotated[str, Parameter(choices=tuple(SHAPES))],
	target: str = GATE_TARGET,
	counts: Annotated[
		tuple[int, ...], Parameter(consume_multiple=True, negative_iterable=(), validator=at_least_one_count)
	] = COUNTS,
) -> int:
	rows = [
		measure(BACKENDS[backend], shape, target, profile, count) for profile in PROFILES for count in counts
	]
	path = results_path(target, shape, backend)
	path.parent.mkdir(parents=True, exist_ok=True)
	with path.open("w", newline="") as handle:
		writer = csv.writer(handle)
		writer.writerow(Measurement._fields)
		writer.writerows(rows)
	for row in rows:
		print(f"{row.backend} {row.shape} {row.target} {row.profile} n={row.message_count} flash={row.flash}")
	return 0


def results_path(target: str, shape: str, backend: str) -> Path:
	return RESULTS / f"{target}-{shape}-{backend}.csv"


if __name__ == "__main__":
	app()
