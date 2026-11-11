"""R-132: no cbor-static ELF the harness builds for the gate target links a ``core::panicking`` symbol.

Every ELF read must show engine machinery, so a broken symbol reader cannot pass, and a control
that indexes out of bounds must show ``core::panicking``, so a build that hides panic paths cannot.
"""

import sys
from typing import TYPE_CHECKING, Final

from harness.features import built
from harness.measure import CRATES, GATE_TARGET, HARNESS, PROFILES, build, elf_label, elf_path, write_crate
from harness.symbols import FIXTURES, MACHINERY, fixture, matched, names
from harness.templates import render
from harness.unused import control, gate

if TYPE_CHECKING:
	from pathlib import Path

PANICKING: Final = "core::panicking"
CONTROL: Final = "panicking-control"


def main() -> int:
	groups = {
		"gate": gate(),
		"R-006 fixtures": tuple(fixture(name) for name in FIXTURES),
		"R-009 positive control": (control(),),
		"R-009 feature cost fixtures (when built)": built(),
	}
	read = {elf: names(elf) for elfs in groups.values() for elf in elfs}
	panicking = sorted(
		f"{elf_label(elf)}: {symbol}"
		for elf, found in read.items()
		for symbol in found
		if PANICKING in symbol
	)
	blind = sorted(elf_label(elf) for elf, found in read.items() if not matched(found, MACHINERY))
	controls = {
		elf_label(elf): sorted(s for s in names(elf) if PANICKING in s) for elf in panicking_controls()
	}
	passed = bool(groups["gate"]) and not panicking and not blind and all(controls.values())
	text = "\n".join(
		(
			f"## R-132 panic symbols: {'PASS' if passed else 'FAIL'}",
			"",
			"| ELFs | count |",
			"|---|---:|",
			*(f"| {group} | {len(elfs)} |" for group, elfs in groups.items()),
			"",
			"| check | result |",
			"|---|---|",
			f"| `{PANICKING}` in the {len(read)} ELFs above | {', '.join(panicking) or 'none ✅'} |",
			f"| engine machinery found in each (reader control) | {'missing in ' + ', '.join(blind) if blind else 'all ✅'} |",
			*(
				f"| `{PANICKING}` in the out-of-bounds control {name} | {', '.join(found) or 'none ❌'} |"
				for name, found in controls.items()
			),
		)
	)
	(HARNESS / "panics.md").write_text(text + "\n")
	print(text)
	return 0 if passed else 1


def panicking_controls() -> tuple[Path, ...]:
	crate = write_crate(
		CRATES / CONTROL,
		{
			"Cargo.toml": render("Cargo.toml", name=CONTROL, dependencies=""),
			"src/main.rs": render("panicking.rs"),
		},
	)
	for profile in PROFILES:
		build(crate, GATE_TARGET, profile, (), HARNESS / CONTROL)
	return tuple(elf_path(HARNESS / CONTROL, GATE_TARGET, profile, CONTROL) for profile in PROFILES)


if __name__ == "__main__":
	sys.exit(main())
