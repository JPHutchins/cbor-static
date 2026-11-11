"""The D-06 size gate over the swept results, plus the informational R-007 comparisons and a report."""

import csv
import sys
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.backend import ROOT
from harness.backends.cbor_static import CBOR_STATIC, CBOR_STATIC_ONESHOT, CBOR_STATIC_REPR
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS
from harness.elf import symbols
from harness.fit import Fit, fits
from harness.generate import crate_name
from harness.measure import GATE_TARGET, HARNESS, PROFILES, Measurement, elf_path, target_dir
from harness.model import SHAPES
from harness.sweep import RESULTS

if TYPE_CHECKING:
	from collections.abc import Callable, Iterable

MARGINAL_REDUCTION: Final = 0.30
FIXED_ALLOWANCE: Final = 2048
OVERHEAD_LIMIT: Final = 0.10
PROFILE_LABELS: Final = {"release": "z", "release-s": "s"}


class Verdict(NamedTuple):
	shape: str
	profile: str
	marginal: float
	baseline_marginal: float
	fixed: float
	baseline_fixed: float

	@property
	def reduction(self) -> float:
		return 1 - self.marginal / self.baseline_marginal

	@property
	def passes(self) -> bool:
		"""
		>>> Verdict("rich", "release", 700.0, 1000.0, 9000.0, 7000.0).passes
		True
		>>> Verdict("rich", "release", 701.0, 1000.0, 9000.0, 7000.0).passes
		False
		>>> Verdict("rich", "release", 500.0, 1000.0, 9049.0, 7000.0).passes
		False
		"""
		return self.reduction >= MARGINAL_REDUCTION and self.fixed <= self.baseline_fixed + FIXED_ALLOWANCE


def main() -> int:
	text, passed = report(load(), lambda m: m.flash)
	(HARNESS / "report.md").write_text(text + "\n")
	print(text)
	return 0 if passed else 1


def load() -> list[Measurement]:
	return [
		Measurement(
			backend=row["backend"],
			shape=row["shape"],
			target=row["target"],
			profile=row["profile"],
			message_count=int(row["message_count"]),
			flash=int(row["flash"]),
			research=int(row["research"]),
			rustc=row["rustc"],
		)
		for path in sorted(RESULTS.glob("*.csv"))
		for row in csv.DictReader(path.read_text().splitlines())
	]


def report(rows: list[Measurement], measure: Callable[[Measurement], int]) -> tuple[str, bool]:
	found = fits(rows, measure)
	results = verdicts(found)
	compilers = {
		m.rustc
		for m in rows
		if m.target == GATE_TARGET and m.backend in (CBOR_STATIC.name, MINICBOR_HAND_TEXTKEYS.name)
	}
	passed = len(compilers) == 1 and all(isinstance(v, Verdict) and v.passes for v in results)
	text = "\n\n".join(
		(
			f"## D-06 gate: {'PASS' if passed else 'FAIL'}",
			f"Flash = every allocated PROGBITS section; {GATE_TARGET}; gated and baseline built by: {' AND '.join(sorted(compilers))}",
			verdict_table(results),
			f"### Fits, {GATE_TARGET}",
			fit_table(found, GATE_TARGET),
			f"### Fits, research measure (.text + .rodata + .data), {GATE_TARGET}",
			fit_table(fits(rows, lambda m: m.research), GATE_TARGET),
			"### Glue enums vs `#[repr(C, u8)]` tables (D-21), marginal",
			comparison(found, CBOR_STATIC_REPR.name, CBOR_STATIC.name, "repr vs glue"),
			f"### Resumability overhead (D-16), marginal; over {OVERHEAD_LIMIT:.0%} means ask the user",
			comparison(found, CBOR_STATIC.name, CBOR_STATIC_ONESHOT.name, "resumable vs one-shot"),
			"### Fits, thumbv6m-none-eabi (informational)",
			fit_table(found, "thumbv6m-none-eabi"),
			f"### Fixed cost: largest symbols at N=1, flat, opt-level z, {GATE_TARGET}",
			breakdown(),
		)
	)
	return text, passed


def breakdown() -> str:
	return "\n".join(
		(
			"| backend | symbol | bytes |",
			"|---|---|---:|",
			*(
				f"| {backend.name} | `{symbol.name[:90]}` | {symbol.size:,} |"
				for backend in (CBOR_STATIC, MINICBOR_HAND_TEXTKEYS)
				for symbol in sorted(
					symbols(
						ROOT,
						elf_path(
							target_dir(backend.name, "flat", GATE_TARGET),
							GATE_TARGET,
							"release",
							crate_name("flat", backend, 1),
						),
					),
					key=lambda s: s.size,
					reverse=True,
				)[:12]
			),
		)
	)


def verdicts(found: list[Fit]) -> tuple[Verdict | str, ...]:
	return tuple(
		Verdict(shape, profile, gated.per_type, baseline.per_type, gated.intercept, baseline.intercept)
		if (gated := find(found, CBOR_STATIC.name, shape, profile))
		and (baseline := find(found, MINICBOR_HAND_TEXTKEYS.name, shape, profile))
		else f"missing {CBOR_STATIC.name} or {MINICBOR_HAND_TEXTKEYS.name} for {shape}/{profile}"
		for shape in SHAPES
		for profile in PROFILES
	)


def fit_table(found: list[Fit], target: str) -> str:
	rows = sorted((f for f in found if f.target == target), key=lambda f: (f.shape, f.profile, f.per_type))
	return "\n".join(
		(
			"| shape | opt | backend | fixed (B) | marginal (B/type) | flash @ N=32 (B) |",
			"|---|---|---|---:|---:|---:|",
			*(
				f"| {f.shape} | {PROFILE_LABELS[f.profile]} | {f.backend} | {f.intercept:,.0f} | {f.per_type:,.0f} | {f.flash_n32:,} |"
				for f in rows
			),
		)
	)


def verdict_table(results: tuple[Verdict | str, ...]) -> str:
	return "\n".join(
		(
			f"| shape | opt | marginal: cbor-static / baseline | reduction (need ≥ {MARGINAL_REDUCTION:.0%}) | fixed: cbor-static / baseline + {FIXED_ALLOWANCE:,} | pass |",
			"|---|---|---:|---:|---:|---|",
			*(
				f"| {v.shape} | {PROFILE_LABELS[v.profile]} | {v.marginal:,.0f} / {v.baseline_marginal:,.0f} "
				f"| {v.reduction:.1%} | {v.fixed:,.0f} / {v.baseline_fixed + FIXED_ALLOWANCE:,.0f} | {'✅' if v.passes else '❌'} |"
				if isinstance(v, Verdict)
				else f"| {v} | | | | | ❌ |"
				for v in results
			),
		)
	)


def comparison(found: list[Fit], backend: str, against: str, label: str) -> str:
	return "\n".join(
		(
			f"| shape | opt | {label} |",
			"|---|---|---:|",
			*(
				f"| {shape} | {PROFILE_LABELS[profile]} | {ratio:+.1%} |"
				for shape in SHAPES
				for profile in PROFILES
				if (a := find(found, backend, shape, profile)) and (b := find(found, against, shape, profile))
				for ratio in (a.per_type / b.per_type - 1,)
			),
		)
	)


def find(
	found: Iterable[Fit], backend: str, shape: str, profile: str, target: str = GATE_TARGET
) -> Fit | None:
	return next(
		(f for f in found if (f.backend, f.shape, f.profile, f.target) == (backend, shape, profile, target)),
		None,
	)


if __name__ == "__main__":
	sys.exit(main())
