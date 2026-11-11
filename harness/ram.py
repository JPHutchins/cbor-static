"""R-007: per-type nesting depth, type-derived stack and encoder/decoder state RAM, measured on thumbv7m."""

import struct
import sys
from typing import Final

from harness.backends.backend import ROOT, path_dependency
from harness.backends.cbor_static import CBOR_STATIC
from harness.elf import symbol_bytes, symbols
from harness.generate import messages, render_message
from harness.measure import CRATES, GATE_TARGET, HARNESS, build, elf_path, write_crate
from harness.templates import render

COLUMNS: Final = ("MAX_DEPTH", "stack (B)", "Encoder (B)", "Decoder (B)")


def main() -> int:
	generated = messages(32, "rich")
	crate = write_crate(
		CRATES / "ram",
		{
			"Cargo.toml": render(
				"Cargo.toml", name="ram", dependencies=path_dependency("cbor-static", ROOT / "cbor-static")
			),
			"src/main.rs": render(
				"ram.rs",
				items="\n".join(render_message(CBOR_STATIC, m) for m in generated),
				count=str(len(generated)),
				rows="\n".join(row(m.name, m.lifetime) for m in generated),
			),
		},
	)
	build(crate, GATE_TARGET, "release", (), HARNESS / "ram")
	elf = elf_path(HARNESS / "ram", GATE_TARGET, "release", "ram")
	sizes = struct.iter_unpack(
		"<4I", symbol_bytes(elf, next(s for s in symbols(ROOT, elf) if s.name == "SIZES"))
	)
	print(
		"\n".join(
			(
				f"| rich type ({GATE_TARGET}) | {' | '.join(COLUMNS)} |",
				"|---|" + "---:|" * len(COLUMNS),
				*(
					f"| {m.name} | {' | '.join(f'{n:,}' for n in row_sizes)} |"
					for m, row_sizes in zip(generated, sizes, strict=True)
				),
			)
		)
	)
	return 0


def row(name: str, lifetime: str) -> str:
	ty = f"{name}{lifetime.replace('a', 'static')}"
	return (
		f"    [<{ty} as cbor_static::Encode>::MAX_DEPTH as u32, "
		f"size_of::<<{ty} as cbor_static::Encode>::Stack>() as u32, "
		f"size_of::<cbor_static::Encoder<'static, {ty}>>() as u32, "
		f"size_of::<cbor_static::Decoder<'static, 'static, {ty}>>() as u32],"
	)


if __name__ == "__main__":
	sys.exit(main())
