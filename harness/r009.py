"""R-009's verdict over the swept gate ELFs and the feature fixtures: at least 512 B of fixed-cost
slack in every thumbv7m cell, every feature's ops absent from the gate ELFs and linked by a positive
control, and each feature's cost, which is reported, not gated.
"""

import sys
from typing import Final

from harness.features import FEATURES
from harness.features import load as load_costs
from harness.fit import fits
from harness.gate import FIXED_ALLOWANCE, PROFILE_LABELS, Verdict, load, verdicts
from harness.measure import GATE_TARGET, HARNESS, PROFILES
from harness.unused import check

SLACK: Final = 512
ENGINE: Final = "cbor_static::engine::"


def main() -> int:
	results = verdicts(fits(load(), lambda m: m.flash), GATE_TARGET)
	slacks = tuple(v for v in results if isinstance(v, Verdict))
	unused = check()
	passed = len(slacks) == len(results) and all(slack(v) >= SLACK for v in slacks) and unused.passed
	text = "\n\n".join(
		(
			f"## R-009: {'PASS' if passed else 'FAIL'}",
			f"### Fixed-cost slack, {GATE_TARGET} (need ≥ {SLACK:,} B in every cell)",
			slack_table(slacks),
			"### Zero when unused",
			unused.table(),
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
