"""Build and measure one backend over N message types, both profiles; append rows to the results CSV."""

import argparse
import csv
import sys
from typing import TYPE_CHECKING, Final

from harness.backends import BACKENDS
from harness.measure import GATE_TARGET, HARNESS, PROFILES, Measurement, measure
from harness.model import SHAPES

if TYPE_CHECKING:
	from collections.abc import Sequence
	from pathlib import Path

RESULTS: Final = HARNESS / "results"


def main(argv: Sequence[str]) -> int:
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--backend", choices=list(BACKENDS), required=True)
	parser.add_argument("--shape", choices=list(SHAPES), required=True)
	parser.add_argument("--target", default=GATE_TARGET)
	parser.add_argument("--counts", nargs="+", type=int, default=[1, 2, 4, 8, 16, 32])
	args = parser.parse_args(argv)
	rows = [
		measure(BACKENDS[args.backend], args.shape, args.target, profile, count)
		for profile in PROFILES
		for count in args.counts
	]
	path = results_path(args.target, args.shape, args.backend)
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
	sys.exit(main(sys.argv[1:]))
