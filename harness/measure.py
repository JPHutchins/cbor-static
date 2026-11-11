"""Write a generated crate, build it for a bare-metal target, and measure its flash."""

import os
import subprocess
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.backend import ROOT
from harness.elf import sizes
from harness.generate import crate_name, render_crate
from harness.llvm import rustc

if TYPE_CHECKING:
	from pathlib import Path

	from harness.backends.backend import Backend

HARNESS: Final = ROOT / "target" / "harness"
CRATES: Final = HARNESS / "crates"
PROFILES: Final = ("release", "release-s")
GATE_TARGET: Final = "thumbv7m-none-eabi"
MANGLING: Final = ("-C", "symbol-mangling-version=v0")


class Measurement(NamedTuple):
	backend: str
	shape: str
	target: str
	profile: str
	message_count: int
	flash: int
	research: int
	rustc: str


def measure(backend: Backend, shape: str, target: str, profile: str, count: int) -> Measurement:
	name = crate_name(shape, backend, count)
	crate = write_crate(CRATES / target / name, render_crate(backend, count, shape))
	built = target_dir(backend.name, shape, target)
	build(crate, target, profile, backend.rustflags, built)
	measured = sizes(elf_path(built, target, profile, name))
	return Measurement(
		backend=backend.name,
		shape=shape,
		target=target,
		profile=profile,
		message_count=count,
		flash=measured.flash,
		research=measured.research,
		rustc=rustc(crate, "-V"),
	)


def write_crate(root: Path, files: dict[str, str]) -> Path:
	for relative, content in files.items():
		(root / relative).parent.mkdir(parents=True, exist_ok=True)
		if not (root / relative).is_file() or (root / relative).read_text() != content:
			(root / relative).write_text(content)
	return root


def build(crate: Path, target: str, profile: str, rustflags: tuple[str, ...], target_dir: Path) -> None:
	subprocess.run(
		["cargo", "build", "--quiet", "--target", target, "--profile", profile],
		cwd=crate,
		check=True,
		env={
			**os.environ,
			"CARGO_TARGET_DIR": str(target_dir),
			"CARGO_ENCODED_RUSTFLAGS": "\x1f".join((*MANGLING, *rustflags)),
		},
	)


def elf_path(target_dir: Path, target: str, profile: str, name: str) -> Path:
	return target_dir / target / profile / name


def target_dir(backend: str, shape: str, target: str) -> Path:
	"""One cargo target dir per sweep cell, so cells build in parallel and stay incremental."""
	return HARNESS / target / f"{backend}-{shape}"
