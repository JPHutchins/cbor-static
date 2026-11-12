"""R-121: firmware whose types derive `Encode` alone, and which only encodes, links none of the
decoder. A control whose types derive `Cbor`, and which decodes strictly too, links every part of it,
so the check cannot pass blind.
"""

import sys
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.features import HEAPLESS, dependencies
from harness.measure import CRATES, GATE_TARGET, HARNESS, build, elf_path, write_crate
from harness.symbols import matched, names
from harness.templates import render

if TYPE_CHECKING:
	from pathlib import Path

DECODER: Final = (
	"cbor_static::engine::decode::",
	"cbor_static::engine::leaf::dec_",
	"cbor_static::engine::strict::",
	"cbor_static::engine::skip::",
	"<cbor_static::engine::input::Carry>",
	"cbor_static::engine::text::",
	"cbor_static::engine::raw::store_",
	"cbor_static::engine::capacity::at",
	"cbor_static::engine::capacity::assemble",
	"cbor_static::engine::strings::DEC_",
)


class Direction(NamedTuple):
	"""A firmware's derive and what its entry point does with one `Message`."""

	name: str
	derive: str
	calls: str


ENCODE_ONLY: Final = Direction(
	"encode-only",
	"cbor_static::Encode",
	"    let value: &Message = unsafe { &*(black_box(0x2000_2000usize) as *const Message) };\n"
	"    black_box(cbor_static::encode_into(value, out).ok());",
)
CONTROL: Final = Direction(
	"encode-decode-control",
	"cbor_static::Cbor",
	"    let input: &[u8] = unsafe { core::slice::from_raw_parts(black_box(0x2000_0000usize) as *const u8, black_box(256)) };\n"
	"    let mut slot = core::mem::MaybeUninit::<Message>::uninit();\n"
	"    if let Ok(value) = cbor_static::decode_strict_into(input, &mut slot) {\n"
	"        black_box(cbor_static::encode_into(&*value, out).ok());\n"
	"    }",
)


def main() -> int:
	leaked = matched(names(firmware(ENCODE_ONLY)), DECODER)
	unlinked = sorted(set(DECODER) - set(matched(names(firmware(CONTROL)), DECODER)))
	passed = not leaked and not unlinked
	text = "\n".join(
		(
			f"## R-121, encode-only firmware links no decoder: {'PASS' if passed else 'FAIL'}",
			"",
			"| check | result |",
			"|---|---|",
			f"| {len(DECODER)} decoder symbol patterns in the encode-only firmware | {', '.join(leaked) or 'none ✅'} |",
			f"| the same in the control, which also decodes | {'missing ' + ', '.join(unlinked) if unlinked else f'all {len(DECODER)} ✅'} |",
		)
	)
	(HARNESS / "encode_only.md").write_text(text + "\n")
	print(text)
	return 0 if passed else 1


def firmware(direction: Direction) -> Path:
	name = f"r121-{direction.name}"
	build(
		write_crate(
			CRATES / name,
			{
				"Cargo.toml": render("Cargo.toml", name=name, dependencies=dependencies((HEAPLESS,))),
				"src/main.rs": render("directions.rs", derive=direction.derive, calls=direction.calls),
			},
		),
		GATE_TARGET,
		"release",
		(),
		HARNESS / "r121",
	)
	return elf_path(HARNESS / "r121", GATE_TARGET, "release", name)


if __name__ == "__main__":
	sys.exit(main())
