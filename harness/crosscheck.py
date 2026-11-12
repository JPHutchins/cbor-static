"""R-004: for every generated message (flat and rich, N = 32), cbor-static and minicbor-hand-textkeys
produce byte-identical CBOR and each decodes the other's bytes.
"""

import os
import subprocess
import sys
from typing import Final

from harness.backends.backend import ROOT, minicbor_source, path_dependency
from harness.backends.cbor_static import CBOR_STATIC
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS
from harness.generate import messages, render_message
from harness.measure import CRATES, HARNESS, write_crate
from harness.model import SHAPES
from harness.templates import render
from harness.values import value

COUNT: Final = 32


def main() -> int:
	crate = write_crate(
		CRATES / "crosscheck",
		{
			"Cargo.toml": render(
				"Cargo.toml",
				name="crosscheck",
				dependencies="\n".join(
					(
						path_dependency("minicbor", minicbor_source() / "minicbor"),
						path_dependency("cbor-static", ROOT / "cbor-static"),
					)
				),
			),
			"src/lib.rs": "#![allow(dead_code)]\n" + "".join(map(module, SHAPES)),
			"tests/wire.rs": render("crosscheck_wire.rs", tests="\n".join(map(tests, SHAPES))),
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
	return f"pub mod {shape} {{\npub mod hand {{\n{hand}\n}}\npub mod stat {{\n{stat}\n}}\n}}\n"


def tests(shape: str) -> str:
	return "\n".join(
		f"#[test]\nfn {shape}_{m.name.lower()}() {{\n"
		+ "".join(
			f"    agree!({shape}::hand::{m.name}{m.lifetime.replace('a', '_')}, {shape}::stat::{m.name}{m.lifetime.replace('a', '_')}, "
			f"{value(m, variant, f'{shape}::hand')}, {value(m, variant, f'{shape}::stat')});\n"
			for variant in range(3)
		)
		+ "}\n"
		for m in messages(COUNT, shape)
	)


if __name__ == "__main__":
	sys.exit(main())
