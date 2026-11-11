"""R-009's verdict over the swept gate ELFs and the feature fixtures: at least 512 B of fixed-cost
slack in every thumbv7m cell, every feature's ops absent from the gate ELFs and linked by a positive
control, and each feature's cost, which is reported, not gated.
"""

import sys
from typing import Final

from harness.backends.cbor_static import CBOR_STATIC
from harness.features import FEATURES, fixture
from harness.features import load as load_costs
from harness.fit import fits
from harness.gate import FIXED_ALLOWANCE, PROFILE_LABELS, Verdict, load, verdicts
from harness.generate import crate_name
from harness.measure import GATE_TARGET, HARNESS, PROFILES, elf_path, target_dir
from harness.symbols import names

SLACK: Final = 512
CONTROL: Final = "r009-positive-control"
ENGINE: Final = "cbor_static::engine::"


def main() -> int:
	rows = load()
	results = verdicts(fits(rows, lambda m: m.flash), GATE_TARGET)
	slacks = tuple(v for v in results if isinstance(v, Verdict))
	anchors = frozenset(symbol for feature in FEATURES for symbol in feature.symbols)
	gate_elfs = tuple(
		elf_path(
			target_dir(m.backend, m.shape, m.target),
			m.target,
			m.profile,
			crate_name(m.shape, CBOR_STATIC, m.message_count),
		)
		for m in rows
		if m.backend == CBOR_STATIC.name and m.target == GATE_TARGET
	)
	leaked = sorted({f"{elf.name}: {symbol}" for elf in gate_elfs for symbol in names(elf) & anchors})
	control = names(
		elf_path(
			fixture(CONTROL, tuple(feature.template for feature in FEATURES), "positive", "release"),
			GATE_TARGET,
			"release",
			CONTROL,
		)
	)
	unlinked = sorted(anchors - control)
	passed = (
		len(slacks) == len(results)
		and all(slack(v) >= SLACK for v in slacks)
		and bool(gate_elfs)
		and not leaked
		and not unlinked
	)
	text = "\n\n".join(
		(
			f"## R-009: {'PASS' if passed else 'FAIL'}",
			f"### Fixed-cost slack, {GATE_TARGET} (need ≥ {SLACK:,} B in every cell)",
			slack_table(slacks),
			"### Zero when unused",
			"\n".join(
				(
					"| check | result |",
					"|---|---|",
					(
						f"| {len(anchors)} feature ops statics in the {len(gate_elfs)} gate ELFs | "
						f"{', '.join(leaked) or 'none ✅'} |"
					),
					(
						f"| the same statics in one positive-control ELF of every feature | "
						f"{'missing ' + ', '.join(unlinked) if unlinked else f'all {len(anchors)} ✅'} |"
					),
				)
			),
			f"### Cost when used, {GATE_TARGET}: fixture minus reference (reported, not gated)",
			cost_table(),
		)
	)
	(HARNESS / "r009.md").write_text(text + "\n")
	print(text)
	return 0 if passed else 1


def slack(verdict: Verdict) -> float:
	"""
	>>> slack(Verdict("rich", "release", 500.0, 1000.0, 8000.0, 6600.0))
	648.0
	"""
	return verdict.baseline_fixed + FIXED_ALLOWANCE - verdict.fixed


def slack_table(slacks: tuple[Verdict, ...]) -> str:
	return "\n".join(
		(
			f"| shape | opt | fixed (B) | ceiling: baseline + {FIXED_ALLOWANCE:,} | slack (B) | pass |",
			"|---|---|---:|---:|---:|---|",
			*(
				f"| {v.shape} | {PROFILE_LABELS[v.profile]} | {v.fixed:,.0f} | "
				f"{v.baseline_fixed + FIXED_ALLOWANCE:,.0f} | {slack(v):,.0f} | {'✅' if slack(v) >= SLACK else '❌'} |"
				for v in slacks
			),
		)
	)


def cost_table() -> str:
	costs = {(cost.feature, cost.profile): cost for cost in load_costs()}
	return "\n".join(
		(
			"| feature | reference | "
			+ " | ".join(
				f"fixed, N=1 ({PROFILE_LABELS[p]}) | per type, N=1→8 ({PROFILE_LABELS[p]})" for p in PROFILES
			)
			+ " | ops statics |",
			"|---|---|" + "---:|---:|" * len(PROFILES) + "---|",
			*(
				f"| {feature.name} | `{feature.reference}` | "
				+ " | ".join(
					f"{costs[feature.name, p].fixed:+,} | {costs[feature.name, p].per_type:+,.1f}"
					for p in PROFILES
				)
				+ f" | {', '.join(f'`{symbol.removeprefix(ENGINE)}`' for symbol in feature.symbols)} |"
				for feature in FEATURES
			),
		)
	)


if __name__ == "__main__":
	sys.exit(main())
