"""Every function's stack frame in the rich fixture at its deepest nesting, built on the pinned nightly
with ``-Z emit-stack-sizes``, against the checked-in baseline: any difference fails until the
baseline is regenerated.
"""

import argparse
import json
import subprocess
import sys
import tomllib
from collections import Counter
from typing import TYPE_CHECKING, Final

from harness.backends.backend import ROOT
from harness.backends.cbor_static import CBOR_STATIC
from harness.features import NIGHTLY, pinned
from harness.generate import crate_name, render_crate
from harness.llvm import llvm_tool
from harness.measure import GATE_TARGET, HARNESS, PROFILES, build, elf_path, write_crate
from harness.sweep import COUNTS

if TYPE_CHECKING:
	from collections.abc import Sequence
	from pathlib import Path

type Frames = dict[str, dict[str, int]]

BASELINE: Final = ROOT / "harness" / "stack-sizes.toml"
STACK: Final = HARNESS / "stack"
SHAPE: Final = "rich"
DEEPEST: Final = max(COUNTS)


def main(argv: Sequence[str]) -> int:
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("mode", choices=list(MODES))
	return MODES[parser.parse_args(argv).mode]()


def fix() -> int:
	BASELINE.write_text(render(measured()))
	return 0


def check() -> int:
	changes = drift(tomllib.loads(BASELINE.read_text()), measured())
	print(
		"\n".join((*changes, "stack frames moved: regenerate the baseline with `camas stack.fix`"))
		if changes
		else f"{BASELINE.name}: unchanged"
	)
	return int(bool(changes))


MODES: Final = {"fix": fix, "check": check}


def measured() -> Frames:
	crate = write_crate(
		STACK / "crates" / crate_name(SHAPE, CBOR_STATIC, DEEPEST),
		{**render_crate(CBOR_STATIC, DEEPEST, SHAPE), **pinned((NIGHTLY,))},
	)
	return {profile: frames(crate, built(crate, profile)) for profile in PROFILES}


def built(crate: Path, profile: str) -> Path:
	build(crate, GATE_TARGET, profile, ("-Z", "emit-stack-sizes"), STACK)
	return elf_path(STACK, GATE_TARGET, profile, crate.name)


def frames(toolchain_dir: Path, elf: Path) -> dict[str, int]:
	return unique(
		tuple(
			(named(entry["Entry"]["Functions"]), entry["Entry"]["Size"])
			for entry in json.loads(
				subprocess.run(
					[
						str(llvm_tool(toolchain_dir, "llvm-readobj")),
						"--elf-output-style=JSON",
						"--stack-sizes",
						"--demangle",
						str(elf),
					],
					capture_output=True,
					text=True,
					check=True,
				).stdout
			)[0]["StackSizes"]
			if not entry["Entry"]["Functions"][0].startswith("OUTLINED_FUNCTION_")
		)
	)


def named(functions: list[str]) -> str:
	"""
	>>> named(["f"])
	'f'
	>>> named(["f", "g"])
	Traceback (most recent call last):
	ValueError: one stack frame under several names: ['f', 'g']

	Raises:
		ValueError: If symbols alias one function.
	"""
	match functions:
		case [function]:
			return function
		case _:
			raise ValueError(f"one stack frame under several names: {functions}")


def unique(sizes: tuple[tuple[str, int], ...]) -> dict[str, int]:
	"""
	>>> unique((("f", 8), ("g", 16)))
	{'f': 8, 'g': 16}
	>>> unique((("f", 8), ("g", 16), ("f", 24)))
	Traceback (most recent call last):
	ValueError: several stack frames under one name: ['f']

	Raises:
		ValueError: If demangled names collide.
	"""
	if duplicated := sorted(name for name, count in Counter(name for name, _ in sizes).items() if count > 1):
		raise ValueError(f"several stack frames under one name: {duplicated}")
	return dict(sizes)


def render(measured: Frames) -> str:
	"""One table per profile, its symbols sorted.

	>>> sample = {"release": {"b": 8, "<a as c::D>::e": 16}, "release-s": {"_start": 832}}
	>>> print(render(sample), end="")
	[release]
	"<a as c::D>::e" = 16
	"b" = 8
	<BLANKLINE>
	[release-s]
	"_start" = 832
	>>> tomllib.loads(render(sample)) == sample
	True
	"""
	return "\n".join(
		f"[{profile}]\n"
		+ "".join(f"{json.dumps(symbol)} = {size}\n" for symbol, size in sorted(table.items()))
		for profile, table in measured.items()
	)


def drift(baseline: Frames, measured: Frames) -> tuple[str, ...]:
	"""Every symbol whose frame differs, appears or disappears, per profile.

	>>> drift({"release": {"f": 8, "g": 16}}, {"release": {"f": 1032, "h": 24}})
	('release f: 8 -> 1032 B (+1024)', 'release g: 16 B -> absent', 'release h: absent -> 24 B')
	>>> drift({"release": {"f": 8}}, {"release": {"f": 8}})
	()
	"""
	return tuple(
		change(profile, symbol, baseline.get(profile, {}).get(symbol), measured.get(profile, {}).get(symbol))
		for profile in sorted(baseline.keys() | measured.keys())
		for symbol in sorted(baseline.get(profile, {}).keys() | measured.get(profile, {}).keys())
		if baseline.get(profile, {}).get(symbol) != measured.get(profile, {}).get(symbol)
	)


def change(profile: str, symbol: str, old: int | None, new: int | None) -> str:
	match old, new:
		case int(), int():
			return f"{profile} {symbol}: {old} -> {new} B ({new - old:+})"
		case int(), None:
			return f"{profile} {symbol}: {old} B -> absent"
		case _:
			return f"{profile} {symbol}: absent -> {new} B"


if __name__ == "__main__":
	sys.exit(main(sys.argv[1:]))
