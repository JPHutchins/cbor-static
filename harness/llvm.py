"""LLVM binutils shipped in a rustup toolchain's sysroot (component ``llvm-tools``)."""

import subprocess
from functools import cache
from pathlib import Path


@cache
def sysroot(toolchain: str) -> Path:
	return Path(
		subprocess.run(
			["rustc", "+" + toolchain, "--print", "sysroot"], capture_output=True, text=True, check=True
		).stdout.strip()
	)


@cache
def host(toolchain: str) -> str:
	return subprocess.run(
		["rustc", "+" + toolchain, "--print", "host-tuple"], capture_output=True, text=True, check=True
	).stdout.strip()


def llvm_tool(toolchain: str, name: str) -> Path:
	return sysroot(toolchain) / "lib" / "rustlib" / host(toolchain) / "bin" / name
