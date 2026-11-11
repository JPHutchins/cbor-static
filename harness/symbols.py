"""Symbol checks: the engine machinery is linked into every gate ELF (D-06), and a scalars-only firmware
links no float, tag, bignum or string handlers while a positive control links them all (D-22, R-006).
"""

import sys
from typing import TYPE_CHECKING, Final

from harness.backends.backend import ROOT, path_dependency
from harness.backends.cbor_static import CBOR_STATIC
from harness.elf import symbols
from harness.gate import load
from harness.generate import crate_name
from harness.measure import CRATES, GATE_TARGET, HARNESS, build, elf_path, target_dir, write_crate
from harness.templates import render

if TYPE_CHECKING:
	from pathlib import Path

MACHINERY: Final = (
	"cbor_static::engine::encode::resume",
	"cbor_static::engine::decode::feed",
	"cbor_static::engine::decode::drop_initialized",
	"<cbor_static::engine::skip::Skip>::on_header",
	"<cbor_static::engine::input::Carry>::header",
	"<cbor_static::engine::pending::Pending>::drain",
	"<cbor_static::engine::text::KeyMatch>::feed",
	"<cbor_static::engine::text::Utf8>::feed",
)
OPTIONAL_KINDS: Final = (
	"enc_scalar::<f32>",
	"dec_scalar::<f32>",
	"enc_scalar::<f64>",
	"dec_scalar::<f64>",
	"enc_tagged",
	"dec_tagged",
	"enc_u128",
	"dec_u128",
	"enc_i128",
	"dec_i128",
)
FIXTURES: Final = {
	"scalars": "pub a: u32, pub b: u64, pub c: i32, pub d: bool, "
	'#[serde(skip_serializing_if = "Option::is_none")] pub e: Option<u32>,',
	"numbers": "pub f: f64, pub g: f32, pub t: cbor_static::Tagged<1, u64>, pub h: u128, pub i: i128,",
}


def machinery() -> int:
	rows = [m for m in load() if m.backend == CBOR_STATIC.name and m.target == GATE_TARGET]
	failures = [
		f"{elf.name} ({m.profile}) lacks {', '.join(sorted(set(MACHINERY) - set(matched(names(elf), MACHINERY))))}"
		for m in rows
		for elf in (
			elf_path(
				target_dir(m.backend, m.shape, m.target),
				m.target,
				m.profile,
				crate_name(m.shape, CBOR_STATIC, m.message_count),
			),
		)
		if len(matched(names(elf), MACHINERY)) < len(MACHINERY)
	]
	print("\n".join(failures) or f"machinery present in all {len(rows)} gate ELFs: {', '.join(MACHINERY)}")
	return 1 if failures or not rows else 0


def fixtures() -> int:
	scalars, numbers = fixture("scalars"), fixture("numbers")
	forbidden = matched(
		scalars, (*OPTIONAL_KINDS, "enc_str", "dec_str", "enc_bytes", "dec_bytes", "core::fmt")
	)
	absent = sorted(set(OPTIONAL_KINDS) - set(matched(numbers, OPTIONAL_KINDS)))
	table = "\n".join(
		(
			"| fixture | check | result |",
			"|---|---|---|",
			f"| scalars-only | no float, tag, bignum, str/bytes or core::fmt symbols | {', '.join(forbidden) or 'none ✅'} |",
			f"| numbers (positive control) | every float, tag and bignum handler linked | {', '.join(absent) or 'all ✅'} |",
		)
	)
	(HARNESS / "fixtures.md").write_text(table + "\n")
	print(table)
	return 1 if forbidden or absent else 0


def fixture(name: str) -> frozenset[str]:
	crate = write_crate(
		CRATES / f"fixture-{name}",
		{
			"Cargo.toml": render(
				"Cargo.toml",
				name=f"fixture-{name}",
				dependencies=path_dependency("cbor-static", ROOT / "cbor-static"),
			),
			"src/main.rs": render("fixture.rs", fields=FIXTURES[name]),
		},
	)
	build(crate, GATE_TARGET, "release", (), HARNESS / "fixtures")
	return names(elf_path(HARNESS / "fixtures", GATE_TARGET, "release", f"fixture-{name}"))


def matched(names: frozenset[str], patterns: tuple[str, ...]) -> tuple[str, ...]:
	"""
	>>> matched(frozenset({"a::enc_tagged::<1, u64>", "b::feed"}), ("enc_tagged", "dec_tagged"))
	('enc_tagged',)
	"""
	return tuple(pattern for pattern in patterns if any(pattern in name for name in names))


def names(elf: Path) -> frozenset[str]:
	return frozenset(symbol.name for symbol in symbols(ROOT, elf))


if __name__ == "__main__":
	sys.exit({"machinery": machinery, "fixtures": fixtures}[sys.argv[1]]())
