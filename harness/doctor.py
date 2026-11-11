"""Verify that every prerequisite of the camas tasks is installed."""

import argparse
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, NamedTuple

from harness.llvm import host, llvm_tool

if TYPE_CHECKING:
	from collections.abc import Sequence

SUBMODULE_MANIFESTS = (
	Path("tests/fixtures/minicbor/minicbor/Cargo.toml"),
	Path("tests/fixtures/minicbor/minicbor-serde/Cargo.toml"),
	Path("tests/fixtures/postcard/source/postcard/Cargo.toml"),
)
STABLE_COMPONENTS = ("clippy", "rustfmt", "llvm-tools")
NIGHTLY_COMPONENTS = ("miri", "rust-src")


class Missing(NamedTuple):
	what: str
	remedy: str


def installed(toolchain: str, listing: str) -> frozenset[str]:
	result = subprocess.run(
		["rustup", "+" + toolchain, listing, "list", "--installed"],
		capture_output=True,
		text=True,
		check=False,
	)
	return frozenset(
		line.removesuffix("-" + host(toolchain)).removesuffix("-preview") for line in result.stdout.split()
	)


def missing(stable: str, nightly: str, targets: Sequence[str]) -> tuple[Missing, ...]:
	stable_targets = installed(stable, "target")
	stable_components = installed(stable, "component")
	nightly_components = installed(nightly, "component")
	return (
		*(
			Missing(f"target {t} on {stable}", f"rustup +{stable} target add {t}")
			for t in targets
			if t not in stable_targets
		),
		*(
			Missing(f"component {c} on {stable}", f"rustup +{stable} component add {c}")
			for c in STABLE_COMPONENTS
			if c not in stable_components
		),
		*(
			Missing(f"component {c} on {nightly}", f"rustup toolchain install {nightly} --component {c}")
			for c in NIGHTLY_COMPONENTS
			if c not in nightly_components
		),
		*(
			Missing(f"submodule {m.parts[2]}", "git submodule update --init")
			for m in SUBMODULE_MANIFESTS
			if not m.is_file()
		),
		*(
			Missing(f"{tool} in the {stable} sysroot", f"rustup +{stable} component add llvm-tools")
			for tool in ("llvm-size", "llvm-nm")
			if not llvm_tool(stable, tool).is_file()
		),
	)


def main(argv: Sequence[str]) -> int:
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--stable", required=True)
	parser.add_argument("--nightly", required=True)
	parser.add_argument("--targets", nargs="+", required=True)
	args = parser.parse_args(argv)
	found = missing(args.stable, args.nightly, args.targets)
	for m in found:
		print(f"missing {m.what}: {m.remedy}")
	return 1 if found else 0


if __name__ == "__main__":
	sys.exit(main(sys.argv[1:]))
