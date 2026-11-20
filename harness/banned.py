"""Symbols no cbor-static ELF the harness builds may link: ``core::panicking`` (R-132, D-24) and
compiler-builtins floating-point routines (R-135, D-22a); nor may the serde layer's link ``core::fmt``
(D-23).

Every ELF read must show engine machinery, so a broken symbol reader cannot pass, and each rule's
control firmware must link a banned symbol, so a build that hides them cannot pass either.
"""

import re
import sys
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.cbor_static_serde import CBOR_STATIC_SERDE
from harness.features import BY_TEMPLATE, built, fixture_name
from harness.features import fixture as feature_fixture
from harness.gate import INFORMATIONAL_TARGET, load
from harness.measure import (
	CRATES,
	GATE_TARGET,
	HARNESS,
	PROFILES,
	build,
	elf_label,
	elf_path,
	measured_elf,
	write_crate,
)
from harness.symbols import FIXTURES, MACHINERY, fixture, matched, names
from harness.templates import render
from harness.unused import allocating, control, gate, serde, tokens

if TYPE_CHECKING:
	from collections.abc import Callable, Mapping
	from pathlib import Path

SERDE: Final = "serde layer"
SERDE_MACHINERY: Final = (
	"<cbor_static::serde::ser::Serializer>::put",
	"<cbor_static::serde::de::Deserializer>::open_map",
)
SOFT_FLOAT: Final = re.compile(r"__aeabi_(?:[fdh]|c[fd]|u?[il]2[fd])\w*|__[a-z]+[sdh]f[a-z0-9]*")


class Rule(NamedTuple):
	"""A requirement's banned symbols, the report it writes, the control firmwares that must link
	them, the R-009 feature templates exempt from it, each of which must link alloc's error panic,
	and the groups of ELFs it reads, every group when empty.
	"""

	requirement: str
	what: str
	banned: Callable[[str], bool]
	report: str
	controls: tuple[str, ...]
	targets: tuple[str, ...]
	exempt: Mapping[str, tuple[str, ...]] = {}
	within: tuple[str, ...] = ()

	@property
	def excluded(self) -> frozenset[str]:
		"""
		>>> sorted(RULES["panics"].excluded), RULES["soft_float"].excluded
		(['btree_map', 'btree_map_decode', 'btree_max_len'], frozenset())
		"""
		return frozenset(template for templates in self.exempt.values() for template in templates)


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


def formatting(symbol: str) -> bool:
	"""
	>>> formatting("core::fmt::write"), formatting("<u32 as core::fmt::Display>::fmt"), formatting("cbor_static::serde::ser::Serializer::put")
	(True, True, False)
	"""
	return "core::fmt" in symbol


EXEMPT_FROM_PANICS: Final = {"BTreeMap": ("btree_map", "btree_map_decode", "btree_max_len")}
ALLOC_ERROR_PANIC: Final = "core::panicking::panic_nounwind_fmt"

RULES: Final = {
	"panics": Rule(
		"R-132",
		"`core::panicking` symbols",
		panicking,
		"panicking",
		("panicking", "alloc_panicking"),
		(GATE_TARGET,),
		EXEMPT_FROM_PANICS,
	),
	"soft_float": Rule(
		"R-135",
		"soft-float routines",
		soft_float,
		"soft_float",
		("soft_float",),
		(GATE_TARGET, INFORMATIONAL_TARGET),
	),
	"formatting": Rule(
		"D-23",
		"`core::fmt` symbols",
		formatting,
		"formatting",
		("formatting",),
		(GATE_TARGET,),
		within=(SERDE,),
	),
}


def main(rule: Rule) -> int:
	swept = serde_sweep(rule.targets)
	groups = {
		"gate": gate(rule.targets),
		"R-006 fixtures": tuple(fixture(name, target) for name in FIXTURES for target in rule.targets),
		"R-009 positive controls": (
			control(nightly=False, excluded=rule.excluded),
			control(nightly=True, excluded=rule.excluded),
		),
		"token control": (tokens(),),
		"alloc control": (allocating(),),
		"R-009 feature cost fixtures (when built)": built(rule.excluded),
		SERDE: (*swept, *(serde(profile) for profile in PROFILES)),
	}
	groups = {name: elfs for name, elfs in groups.items() if not rule.within or name in rule.within}
	machinery = {
		elf: SERDE_MACHINERY if name == SERDE else MACHINERY for name, elfs in groups.items() for elf in elfs
	}
	read = {elf: names(elf) for elfs in groups.values() for elf in elfs}
	linked = sorted(
		f"{elf_label(elf)}: {symbol}"
		for elf, found in read.items()
		for symbol in found
		if rule.banned(symbol)
	)
	blind = sorted(elf_label(elf) for elf, found in read.items() if not matched(found, machinery[elf]))
	controls = {
		f"{target}/{elf_label(elf)}": sorted(filter(rule.banned, names(elf)))
		for target in rule.targets
		for control in rule.controls
		for elf in controls_of(control, target)
	}
	exempt = {
		f"{name}: {elf_label(elf)}": sorted(filter(rule.banned, names(elf)))
		for name, templates in rule.exempt.items()
		for template in templates
		for elf in exempt_elfs(template)
	}
	passed = (
		("gate" not in groups or bool(groups["gate"]))
		and (SERDE not in groups or bool(swept))
		and not linked
		and not blind
		and all(controls.values())
		and all(ALLOC_ERROR_PANIC in found for found in exempt.values())
	)
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
			*(
				f"| {rule.what} in exempt {name}, which must link `{ALLOC_ERROR_PANIC}`, where "
				f"`handle_alloc_error` ends | {', '.join(found) if ALLOC_ERROR_PANIC in found else 'missing ❌'} |"
				for name, found in exempt.items()
			),
		)
	)
	(HARNESS / f"{rule.report}.md").write_text(text + "\n")
	print(text)
	return 0 if passed else 1


def serde_sweep(targets: tuple[str, ...]) -> tuple[Path, ...]:
	"""Every swept ELF of the serde layer's backend for `targets`."""
	return tuple(
		measured_elf(m) for m in load() if m.backend == CBOR_STATIC_SERDE.name and m.target in targets
	)


def exempt_elfs(template: str) -> tuple[Path, ...]:
	"""An exempt feature's one-type fixture at every profile."""
	feature = BY_TEMPLATE[template]
	return tuple(
		feature_fixture(fixture_name(template, 1), (template,), template, profile, feature.cargo_features)
		for profile in PROFILES
	)


def controls_of(control: str, target: str) -> tuple[Path, ...]:
	"""Builds a rule's control firmware at every profile for `target`."""
	name = f"{control.replace('_', '-')}-control"
	crate = write_crate(
		CRATES / name,
		{
			"Cargo.toml": render("Cargo.toml", name=name, dependencies=""),
			"src/main.rs": render(f"{control}.rs", allocator=render("allocator.rs")),
		},
	)
	for profile in PROFILES:
		build(crate, target, profile, (), HARNESS / name)
	return tuple(elf_path(HARNESS / name, target, profile, name) for profile in PROFILES)


if __name__ == "__main__":
	sys.exit(main(RULES[sys.argv[1]]))
