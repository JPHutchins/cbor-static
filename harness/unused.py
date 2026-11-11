"""Zero when unused (R-009 criterion 2, R-134): no gate ELF links any feature's ops statics, and one
positive control with a type of every feature links them all.
"""

import sys
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.cbor_static import VARIANTS
from harness.features import FEATURES, fixture
from harness.gate import load
from harness.measure import GATE_TARGET, HARNESS, elf_label, measured_elf
from harness.symbols import names

if TYPE_CHECKING:
	from pathlib import Path

CONTROL: Final = "r009-positive-control"


class Unused(NamedTuple):
	anchors: frozenset[str]
	gate_elfs: int
	leaked: tuple[str, ...]
	unlinked: tuple[str, ...]

	@property
	def passed(self) -> bool:
		"""
		>>> Unused(frozenset({"a"}), 72, (), ()).passed
		True
		>>> Unused(frozenset({"a"}), 0, (), ()).passed
		False
		>>> Unused(frozenset({"a"}), 72, ("n1: a",), ()).passed
		False
		"""
		return bool(self.gate_elfs) and not self.leaked and not self.unlinked

	def table(self) -> str:
		return "\n".join(
			(
				"| check | result |",
				"|---|---|",
				(
					f"| {len(self.anchors)} feature ops statics in the {self.gate_elfs} gate ELFs | "
					f"{', '.join(self.leaked) or 'none ✅'} |"
				),
				(
					"| the same statics in one positive-control ELF of every feature | "
					f"{'missing ' + ', '.join(self.unlinked) if self.unlinked else f'all {len(self.anchors)} ✅'} |"
				),
			)
		)


def main() -> int:
	found = check()
	text = f"## Zero when unused: {'PASS' if found.passed else 'FAIL'}\n\n{found.table()}"
	(HARNESS / "unused.md").write_text(text + "\n")
	print(text)
	return 0 if found.passed else 1


def check() -> Unused:
	anchors = frozenset(symbol for feature in FEATURES for symbol in feature.symbols)
	gate_elfs = gate()
	return Unused(
		anchors,
		len(gate_elfs),
		tuple(
			sorted({f"{elf_label(elf)}: {symbol}" for elf in gate_elfs for symbol in names(elf) & anchors})
		),
		tuple(sorted(anchors - names(control()))),
	)


def gate() -> tuple[Path, ...]:
	"""Every swept cbor-static ELF of the gate target."""
	return tuple(
		measured_elf(m)
		for m in load()
		if m.backend in {variant.name for variant in VARIANTS} and m.target == GATE_TARGET
	)


def control() -> Path:
	return fixture(CONTROL, tuple(feature.template for feature in FEATURES), "positive", "release")


if __name__ == "__main__":
	sys.exit(main())
