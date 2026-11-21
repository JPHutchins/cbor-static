"""Stack frames of the engine, the rich fixture and its serde twin, against the checked-in baseline."""

import json
import subprocess
import tomllib
from collections import Counter
from typing import TYPE_CHECKING, Final

from cyclopts import App

from harness.backends.backend import ROOT
from harness.backends.cbor_static import CBOR_STATIC
from harness.backends.cbor_static_serde import CBOR_STATIC_SERDE
from harness.features import nightly_pin
from harness.generate import crate_name, render_crate
from harness.llvm import llvm_tool
from harness.measure import COUNTS, CRATES, GATE_TARGET, HARNESS, PROFILES, build, elf_path, write_crate

if TYPE_CHECKING:
	from pathlib import Path

	from harness.backends.backend import Backend

type Frames = dict[str, dict[str, int]]

BASELINE: Final = ROOT / "harness" / "stack-sizes.toml"
STACK: Final = HARNESS / "stack"
SHAPE: Final = "rich"
DEEPEST: Final = max(COUNTS)


app = App(help=__doc__)


@app.default
def unnamed() -> int:
	app.help_print()
	return 2


@app.command
def fix() -> int:
	BASELINE.write_text(render(measured()))
	return 0


@app.command
def check() -> int:
	changes = drift(tomllib.loads(BASELINE.read_text()), measured())
	print(
		"\n".join((*changes, "stack frames moved: regenerate the baseline with `camas stack.fix`"))
		if changes
		else f"{BASELINE.name}: unchanged"
	)
	return int(bool(changes))


def measured() -> Frames:
	"""The native fixture's frames under each profile's name, and the serde layer's (D-15a) under
	the profile's name prefixed with `serde-`.
	"""
	return {
		f"{prefix}{profile}": frames(crate, built(crate, profile))
		for prefix, crate in (("", stack_crate(CBOR_STATIC)), ("serde-", stack_crate(CBOR_STATIC_SERDE)))
		for profile in PROFILES
	}


def stack_crate(backend: Backend) -> Path:
	"""The rich fixture at its deepest count, for `backend`, on the pinned nightly."""
	return write_crate(
		CRATES / "stack" / crate_name(SHAPE, backend, DEEPEST),
		{**render_crate(backend, DEEPEST, SHAPE), **nightly_pin()},
	)


def render(profiles: Frames) -> str:
	"""One table per profile, its symbols sorted.

	>>> sample = {"release": {"b": 8, "<a as c::D>::e": 16}, "release-s": {"_start": 832, "\U0001d465": 8}}
	>>> print(render(sample), end="")
	[release]
	"<a as c::D>::e" = 16
	"b" = 8
	<BLANKLINE>
	[release-s]
	"_start" = 832
	"\U0001d465" = 8
	>>> tomllib.loads(render(sample)) == sample
	True
	"""
	return "\n".join(
		f"[{profile}]\n"
		+ "".join(
			f"{json.dumps(symbol, ensure_ascii=False)} = {size}\n" for symbol, size in sorted(table.items())
		)
		for profile, table in profiles.items()
	)


def drift(baseline: Frames, profiles: Frames) -> tuple[str, ...]:
	"""Every symbol whose frame differs, appears or disappears, per profile.

	>>> drift({"release": {"f": 8, "g": 16}}, {"release": {"f": 1032, "h": 24}})
	('release f: 8 -> 1032 B (+1024)', 'release g: 16 B -> absent', 'release h: absent -> 24 B')
	>>> drift({"release": {"f": 8}}, {"release": {"f": 8}})
	()
	"""
	return tuple(
		change(profile, symbol, baseline.get(profile, {}).get(symbol), profiles.get(profile, {}).get(symbol))
		for profile in sorted(baseline.keys() | profiles.keys())
		for symbol in sorted(baseline.get(profile, {}).keys() | profiles.get(profile, {}).keys())
		if baseline.get(profile, {}).get(symbol) != profiles.get(profile, {}).get(symbol)
	)


def change(profile: str, symbol: str, old: int | None, new: int | None) -> str:
	"""
	>>> change("release", "f", None, None)
	Traceback (most recent call last):
	ValueError: release f: in neither the baseline nor the measurement

	Raises:
		ValueError: If the symbol has no frame on either side.
	"""
	match old, new:
		case int(), int():
			return f"{profile} {symbol}: {old} -> {new} B ({new - old:+})"
		case int(), None:
			return f"{profile} {symbol}: {old} B -> absent"
		case None, int():
			return f"{profile} {symbol}: absent -> {new} B"
		case _:
			raise ValueError(f"{profile} {symbol}: in neither the baseline nor the measurement")


def built(crate: Path, profile: str) -> Path:
	build(crate, GATE_TARGET, profile, ("-Z", "emit-stack-sizes"), STACK)
	return elf_path(STACK, GATE_TARGET, profile, crate.name)


def frames(toolchain_dir: Path, elf: Path) -> dict[str, int]:
	return engine(
		unique(
			tuple(
				(symbol, size)
				for symbol, size in (
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
							stdout=subprocess.PIPE,
							text=True,
							check=True,
						).stdout
					)[0]["StackSizes"]
				)
				if not symbol.startswith("OUTLINED_FUNCTION_")
			)
		)
	)


def engine(sizes: dict[str, int]) -> dict[str, int]:
	"""
	>>> engine({"_start": 832, "cbor_static::engine::decode::value": 128})["_start"]
	832
	>>> engine({})
	Traceback (most recent call last):
	ValueError: no engine stack frame measured: was .stack_sizes emitted and kept?

	Raises:
		ValueError: If no cbor_static function has a frame.
	"""
	if not any("cbor_static::" in symbol for symbol in sizes):
		raise ValueError("no engine stack frame measured: was .stack_sizes emitted and kept?")
	return sizes


def named(functions: list[str]) -> str:
	"""
	>>> named(["f"])
	'f'
	>>> named(["f", "g"])
	Traceback (most recent call last):
	ValueError: a stack frame under other than one name: ['f', 'g']
	>>> named([])
	Traceback (most recent call last):
	ValueError: a stack frame under other than one name: []

	Raises:
		ValueError: If symbols alias one function, or the symbol table is gone.
	"""
	match functions:
		case [function]:
			return function
		case _:
			raise ValueError(f"a stack frame under other than one name: {functions}")


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


if __name__ == "__main__":
	app()
