"""LLVM binutils from the rustup ``llvm-tools`` component of the toolchain a directory selects."""

import subprocess
from functools import cache
from pathlib import Path


def llvm_tool(toolchain_dir: Path, name: str) -> Path:
	return (
		Path(rustc(toolchain_dir, "--print", "sysroot"))
		/ "lib"
		/ "rustlib"
		/ rustc(toolchain_dir, "--print", "host-tuple")
		/ "bin"
		/ name
	)


@cache
def rustc(toolchain_dir: Path, *args: str) -> str:
	return subprocess.run(
		["rustc", *args], cwd=toolchain_dir, capture_output=True, text=True, check=True
	).stdout.strip()
