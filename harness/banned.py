"""Symbols no cbor-static ELF the harness builds may link: ``core::panicking`` (R-132, D-24) and
compiler-builtins floating-point routines (R-135, D-22a).

Every ELF read must show engine machinery, so a broken symbol reader cannot pass, and each rule's
control firmware must link a banned symbol, so a build that hides them cannot pass either.
"""

import re
import sys
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.features import built
from harness.gate import INFORMATIONAL_TARGET
from harness.measure import CRATES, GATE_TARGET, HARNESS, PROFILES, build, elf_label, elf_path, write_crate
from harness.symbols import FIXTURES, MACHINERY, fixture, matched, names
from harness.templates import render
from harness.unused import control, gate, tokens

if TYPE_CHECKING:
	from collections.abc import Callable
	from pathlib import Path

SOFT_FLOAT: Final = re.compile(r"__aeabi_(?:[fdh]|c[fd]|u?[il]2[fd])\w*|__[a-z]+[sdh]f[a-z0-9]*")


class Rule(NamedTuple):
	requirement: str
	what: str
	banned: Callable[[str], bool]
	control: str
	targets: tuple[str, ...]


def panicking(symbol: str) -> bool:
	"""
	>>> panicking("core::panicking::panic_bounds_check"), panicking("cbor_static::engine::decode::feed")
	(True, False)
	"""
	return "core::panicking" in symbol


def soft_float(symbol: str) -> bool:
	"""
	>>> [soft_float(s) for s in ("__aeabi_dmul", "__aeabi_ui2d", "__aeabi_cdcmple", "__aeabi_h2f")]
	[True, True, True, True]
	>>> [soft_float(s) for s in ("compiler_builtins::float::mul::__muldf3", "__extendsfdf2", "__fixdfsi")]
	[True, True, True]
	>>> [soft_float(s) for s in ("__aeabi_uidiv", "__aeabi_ldivmod", "__aeabi_memclr8", "__udivmoddi4")]
	[False, False, False, False]
	>>> soft_float("cbor_static::wire::float::shortest")
	False
	"""
	return bool(SOFT_FLOAT.fullmatch(symbol.rsplit("::", 1)[-1])) or "compiler_builtins::float::" in symbol


RULES: Final = {
	"panics": Rule("R-132", "`core::panicking` symbols", panicking, "panicking", (GATE_TARGET,)),
	"soft_float": Rule(
		"R-135", "soft-float routines", soft_float, "soft_float", (GATE_TARGET, INFORMATIONAL_TARGET)
	),
}


def main(rule: Rule) -> int:
	groups = {
		"gate": gate(rule.targets),
		"R-006 fixtures": tuple(fixture(name, target) for name in FIXTURES for target in rule.targets),
		"R-009 positive controls": (control(nightly=False), control(nightly=True)),
		"token control": (tokens(),),
		"R-009 feature cost fixtures (when built)": built(),
	}
	read = {elf: names(elf) for elfs in groups.values() for elf in elfs}
	linked = sorted(
		f"{elf_label(elf)}: {symbol}"
		for elf, found in read.items()
		for symbol in found
		if rule.banned(symbol)
	)
	blind = sorted(elf_label(elf) for elf, found in read.items() if not matched(found, MACHINERY))
	controls = {
		f"{target}/{elf_label(elf)}": sorted(filter(rule.banned, names(elf)))
		for target in rule.targets
		for elf in controls_of(rule, target)
	}
	passed = bool(groups["gate"]) and not linked and not blind and all(controls.values())
	text = "\n".join(
		(
			f"## {rule.requirement}, {rule.what}: {'PASS' if passed else 'FAIL'}",
			"",
			"| ELFs | count |",
			"|---|---:|",
			*(f"| {group} | {len(elfs)} |" for group, elfs in groups.items()),
			"",
			"| check | result |",
			"|---|---|",
			f"| {rule.what} in the {len(read)} ELFs above | {', '.join(linked) or 'none ✅'} |",
			f"| engine machinery found in each (reader control) | {'missing in ' + ', '.join(blind) if blind else 'all ✅'} |",
			*(
				f"| {rule.what} in the control {name} | {', '.join(found) or 'none ❌'} |"
				for name, found in controls.items()
			),
		)
	)
	(HARNESS / f"{rule.control}.md").write_text(text + "\n")
	print(text)
	return 0 if passed else 1


def controls_of(rule: Rule, target: str) -> tuple[Path, ...]:
	"""Builds the rule's control firmware at every profile for `target`."""
	name = f"{rule.control.replace('_', '-')}-control"
	crate = write_crate(
		CRATES / name,
		{
			"Cargo.toml": render("Cargo.toml", name=name, dependencies=""),
			"src/main.rs": render(f"{rule.control}.rs"),
		},
	)
	for profile in PROFILES:
		build(crate, target, profile, (), HARNESS / name)
	return tuple(elf_path(HARNESS / name, target, profile, name) for profile in PROFILES)


if __name__ == "__main__":
	sys.exit(main(RULES[sys.argv[1]]))
