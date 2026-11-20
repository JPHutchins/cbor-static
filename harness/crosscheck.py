"""R-004: for every generated message (flat and rich, N = 32), cbor-static and minicbor-hand-textkeys
produce byte-identical CBOR and each decodes the other's bytes; and the field vocabulary's longest
encodings, alone and under tags, are minicbor's.

R-302: the same messages, each deriving both cbor-static layers from one definition (D-08a), encode
to identical bytes through both, and each layer decodes the other's.
"""

import os
import subprocess
import sys
from typing import Final

from harness.backends.backend import SERDE, SERDE_BYTES, Backend, no_impls, serde_map_field_attrs
from harness.backends.cbor_static import CBOR_STATIC
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS, minicbor
from harness.features import HEAPLESS, dependencies
from harness.generate import messages, render_message
from harness.measure import CRATES, HARNESS, write_crate
from harness.model import SHAPES, EnumDef
from harness.templates import render
from harness.values import value

COUNT: Final = 32
BOTH_DERIVE: Final = "#[derive(cbor_static::Cbor, serde::Serialize, serde::Deserialize)]"


def both_enum(enum: EnumDef) -> str:
	return f"{BOTH_DERIVE}\npub enum {enum.name} {{ A, B(u32), C {{ x: u32, y: bool }} }}\n"


BOTH: Final = Backend(
	name="cbor-static-both-layers",
	dependencies=lambda: "",
	prelude="",
	struct_attrs=BOTH_DERIVE,
	field_attrs=serde_map_field_attrs,
	enum_def=both_enum,
	impls=no_impls,
	roundtrip=lambda _: "",
)


def main() -> int:
	crate = write_crate(
		CRATES / "crosscheck",
		{
			"Cargo.toml": render(
				"Cargo.toml",
				name="crosscheck",
				dependencies="\n".join(
					(minicbor(("half",)), dependencies((HEAPLESS, "serde")), SERDE, SERDE_BYTES)
				),
			),
			"src/lib.rs": "#![allow(dead_code)]\n" + "".join(map(module, SHAPES)),
			"tests/wire.rs": render("crosscheck_wire.rs", tests="\n".join(map(tests, SHAPES))),
			"tests/vocabulary.rs": render("crosscheck_vocabulary.rs"),
		},
	)
	return subprocess.run(
		["cargo", "test", "--quiet"],
		cwd=crate,
		env={**os.environ, "CARGO_TARGET_DIR": str(HARNESS / "crosscheck")},
		check=False,
	).returncode


def module(shape: str) -> str:
	hand = "\n".join(
		render_message(MINICBOR_HAND_TEXTKEYS, m) + MINICBOR_HAND_TEXTKEYS.impls(m)
		for m in messages(COUNT, shape)
	)
	stat = "\n".join(render_message(CBOR_STATIC, m) for m in messages(COUNT, shape))
	both = "\n".join(render_message(BOTH, m) for m in messages(COUNT, shape))
	return (
		f"pub mod {shape} {{\npub mod hand {{\n{hand}\n}}\npub mod stat {{\n{stat}\n}}\n"
		f"pub mod both {{\n{both}\n}}\n}}\n"
	)


def tests(shape: str) -> str:
	return "\n".join(
		f"#[test]\nfn {shape}_{m.name.lower()}() {{\n"
		+ "".join(
			f"    agree!({shape}::hand::{m.name}{m.lifetime.replace('a', '_')}, {shape}::stat::{m.name}{m.lifetime.replace('a', '_')}, "
			f"{value(m, variant, f'{shape}::hand')}, {value(m, variant, f'{shape}::stat')});\n"
			f"    layers!({shape}::both::{m.name}{m.lifetime.replace('a', '_')}, {value(m, variant, f'{shape}::both')});\n"
			for variant in range(3)
		)
		+ "}\n"
		for m in messages(COUNT, shape)
	)


if __name__ == "__main__":
	sys.exit(main())
