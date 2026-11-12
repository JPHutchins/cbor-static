"""Zero when unused (R-009 criterion 2, R-134): no gate ELF links the anchors of any optional
code, and a control firmware that uses that code links them all.
"""

import sys
from functools import partial
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.backend import ROOT, path_dependency
from harness.backends.cbor_static import VARIANTS
from harness.features import FEATURES, fixture
from harness.gate import load
from harness.measure import (
	CRATES,
	GATE_TARGET,
	HARNESS,
	build,
	elf_label,
	elf_path,
	measured_elf,
	write_crate,
)
from harness.symbols import names
from harness.templates import render

if TYPE_CHECKING:
	from collections.abc import Callable
	from pathlib import Path

CONTROL: Final = "r009-positive-control"
TOKENS: Final = "tokens-control"


class Anchored(NamedTuple):
	"""Optional code, the symbols that show it is linked, and a control firmware that uses it."""

	what: str
	symbols: frozenset[str]
	control: str
	build: Callable[[], Path]


class Found(NamedTuple):
	anchored: Anchored
	leaked: tuple[str, ...]
	unlinked: tuple[str, ...]


class Unused(NamedTuple):
	gate_elfs: int
	found: tuple[Found, ...]

	@property
	def passed(self) -> bool:
		"""
		>>> Unused(72, (Found(ANCHORED[0], (), ()),)).passed
		True
		>>> Unused(0, (Found(ANCHORED[0], (), ()),)).passed
		False
		>>> Unused(72, (Found(ANCHORED[1], ("release/n1: a",), ()),)).passed
		False
		"""
		return bool(self.gate_elfs) and not any(found.leaked or found.unlinked for found in self.found)

	def table(self) -> str:
		return "\n".join(
			(
				"| check | result |",
				"|---|---|",
				*(
					row
					for found in self.found
					for row in (
						(
							f"| {len(found.anchored.symbols)} {found.anchored.what} in the {self.gate_elfs} gate ELFs | "
							f"{', '.join(found.leaked) or 'none ✅'} |"
						),
						(
							f"| the same in {found.anchored.control} | "
							f"{'missing ' + ', '.join(found.unlinked) if found.unlinked else f'all {len(found.anchored.symbols)} ✅'} |"
						),
					)
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
	gate_elfs = gate()
	return Unused(
		len(gate_elfs),
		tuple(
			Found(
				anchored,
				tuple(
					sorted(
						{
							f"{elf_label(elf)}: {symbol}"
							for elf in gate_elfs
							for symbol in names(elf) & anchored.symbols
						}
					)
				),
				tuple(sorted(anchored.symbols - names(anchored.build()))),
			)
			for anchored in ANCHORED
		),
	)


def gate(targets: tuple[str, ...] = (GATE_TARGET,)) -> tuple[Path, ...]:
	"""Every swept cbor-static ELF of `targets`."""
	return tuple(
		measured_elf(m)
		for m in load()
		if m.backend in {variant.name for variant in VARIANTS} and m.target in targets
	)


def control(nightly: bool) -> Path:
	"""One firmware of every feature that builds on the stable toolchain, or on nightly."""
	features = tuple(feature for feature in FEATURES if feature.nightly == nightly)
	return fixture(
		f"{CONTROL}-nightly" if nightly else CONTROL,
		tuple(feature.template for feature in features),
		"positive-nightly" if nightly else "positive",
		"release",
		tuple(sorted({cargo for feature in features for cargo in feature.cargo_features})),
	)


def tokens() -> Path:
	build(
		write_crate(
			CRATES / TOKENS,
			{
				"Cargo.toml": render(
					"Cargo.toml",
					name=TOKENS,
					dependencies=path_dependency("cbor-static", ROOT / "cbor-static"),
				),
				"src/main.rs": render("tokens.rs"),
			},
		),
		GATE_TARGET,
		"release",
		(),
		HARNESS / TOKENS,
	)
	return elf_path(HARNESS / TOKENS, GATE_TARGET, "release", TOKENS)


ANCHORED: Final = (
	*(
		Anchored(
			f"{'nightly ' if nightly else ''}feature ops statics and handlers",
			frozenset(
				symbol for feature in FEATURES if feature.nightly == nightly for symbol in feature.symbols
			),
			f"one positive-control ELF of every {'nightly' if nightly else 'stable'} feature",
			partial(control, nightly=nightly),
		)
		for nightly in (False, True)
	),
	Anchored(
		"token API functions",
		frozenset(
			{
				"<cbor_static::token::Tokens as core::iter::traits::iterator::Iterator>::next",
				"<cbor_static::token::writer::TokenWriter>::write",
			}
		),
		"a firmware that copies tokens from Tokens to a TokenWriter",
		tokens,
	),
)


if __name__ == "__main__":
	sys.exit(main())
