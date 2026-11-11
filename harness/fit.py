"""Least-squares per-message-type slope and intercept of flash."""

from itertools import groupby
from typing import TYPE_CHECKING, NamedTuple

if TYPE_CHECKING:
	from collections.abc import Callable, Iterable

	from harness.measure import Measurement


class Fit(NamedTuple):
	backend: str
	shape: str
	target: str
	profile: str
	intercept: float
	per_type: float
	flash_n32: int


def fits(rows: Iterable[Measurement], flash: Callable[[Measurement], int]) -> list[Fit]:
	def key(m: Measurement) -> tuple[str, str, str, str]:
		return (m.target, m.shape, m.profile, m.backend)

	return [
		fit(group, flash)
		for group in (list(g) for _, g in groupby(sorted(rows, key=key), key=key))
		if len({m.message_count for m in group if m.message_count >= 1}) >= 2
	]


def fit(rows: list[Measurement], flash: Callable[[Measurement], int]) -> Fit:
	"""Ordinary least squares over N >= 1.

	>>> from harness.measure import Measurement
	>>> rows = [Measurement("b", "flat", "t", "release", n, 1000 + 500 * n, 0, "") for n in (0, 1, 2, 32)]
	>>> fit(rows, lambda m: m.flash)[4:]
	(1000.0, 500.0, 17000)
	"""
	points = [(row.message_count, flash(row)) for row in rows if row.message_count >= 1]
	mean_x = sum(x for x, _ in points) / len(points)
	mean_y = sum(y for _, y in points) / len(points)
	slope = sum((x - mean_x) * (y - mean_y) for x, y in points) / sum((x - mean_x) ** 2 for x, _ in points)
	first = rows[0]
	return Fit(
		first.backend,
		first.shape,
		first.target,
		first.profile,
		mean_y - slope * mean_x,
		slope,
		dict(points).get(32, 0),
	)
