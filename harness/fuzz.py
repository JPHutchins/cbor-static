"""The fuzz crate's generated half and its inputs: the harness's flat and rich messages and serde's
forms as a fixtures crate, the seed corpus of each target, and the dependencies it shares with the
differential oracles.
"""

import hashlib
import os
import shutil
import subprocess
import tomllib
from typing import Annotated, Final
lazy from pathlib import Path

from cyclopts import App, Parameter

from harness.backends.backend import serde_bytes_dependency, serde_dependency
from harness.crosscheck import COUNT, VARIANTS, both
from harness.differential import CONFORMANCE, Lock, Manifest
from harness.features import HEAPLESS, dependencies
from harness.generate import messages
from harness.manifests import FuzzManifest
from harness.measure import CRATES, HARNESS, write_crate
from harness.model import SHAPES
from harness.templates import render
from harness.values import value
from harness.workspace import FUZZ_SEEDS, ROOT

FUZZ: Final = ROOT / "fuzz"
FIXTURES: Final = CRATES / "fuzz-fixtures"
GENERATED: Final = "fixtures"


app = App(help=__doc__)


@app.command
def fixtures() -> int:
	"""Write the fixtures crate the fuzz targets read the generated types from."""
	write_crate(FIXTURES, fixture_files())
	return 0


@app.command
def seeds(
	targets: Annotated[
		tuple[str, ...],
		Parameter(
			help="fuzz/Cargo.toml's [[bin]] names; every target when none",
			consume_multiple=True,
			negative_iterable=(),
		),
	] = (),
) -> int:
	"""Gather each target's seed corpus from the vectors its manifest names, one file per input.

	Raises:
		SystemExit: a target names no seeds, or a source holds no input.
	"""
	named = FuzzManifest.read(ROOT).package.metadata.seeds
	if unknown := [target for target in targets if target not in named]:
		raise SystemExit(f"fuzz: fuzz/Cargo.toml names no seeds for {unknown}")
	if any(source.split("/")[0] == GENERATED for target in targets or named for source in named[target]):
		write_seeds()
	for target in targets or tuple(named):
		shutil.rmtree(Path(FUZZ_SEEDS) / target, ignore_errors=True)
		(Path(FUZZ_SEEDS) / target).mkdir(parents=True)
		for source in named[target]:
			if not (inputs := sorted(resolved(source).rglob("*.cbor"))):
				raise SystemExit(f"fuzz: the seed source {source!r} of {target!r} holds no .cbor file")
			for path in inputs:
				contents = path.read_bytes()
				(
					Path(FUZZ_SEEDS) / target / hashlib.sha1(contents, usedforsecurity=False).hexdigest()
				).write_bytes(contents)
	return 0


@app.command
def check() -> int:
	"""The targets' seed sources and the dependencies fuzz/ shares with the differential oracles.

	Raises:
		SystemExit: a target without seeds, seeds for no target, or a dependency that differs.
	"""
	manifest = FuzzManifest.read(ROOT)
	if problems := (
		unseeded(tuple(target.name for target in manifest.targets), tuple(manifest.package.metadata.seeds))
		+ drifted_specs(
			manifest.dependencies,
			Manifest.model_validate(tomllib.loads((CONFORMANCE / "Cargo.toml").read_text())).dev_dependencies,
		)
		+ drifted_locks(
			Lock.model_validate(tomllib.loads((ROOT / "Cargo.lock").read_text())),
			Lock.model_validate(tomllib.loads((FUZZ / "Cargo.lock").read_text())),
		)
	):
		raise SystemExit("\n".join(problems))
	return 0


def unseeded(targets: tuple[str, ...], seeded: tuple[str, ...]) -> tuple[str, ...]:
	"""
	>>> unseeded(("tokens", "oracle"), ("tokens", "gone"))
	("fuzz: the target 'oracle' names no seeds", "fuzz: seeds for 'gone', which is no target")
	"""
	return tuple(
		f"fuzz: the target {target!r} names no seeds" for target in targets if target not in seeded
	) + tuple(f"fuzz: seeds for {name!r}, which is no target" for name in seeded if name not in targets)


def drifted_specs(
	fuzz: dict[str, str | dict[str, object]], oracles: dict[str, str | dict[str, object]]
) -> tuple[str, ...]:
	"""
	>>> drifted_specs(
	...     {"ciborium": "=0.2.2", "libfuzzer-sys": "0.4", "cbor-static": {"path": "a", "features": ["std"]}},
	...     {"ciborium": "=0.2.2", "cddl": "1", "cbor-static": {"path": "a"}},
	... )
	()
	>>> drifted_specs({"ciborium": "=0.2.3"}, {"ciborium": "=0.2.2"})
	("fuzz: fuzz/Cargo.toml depends on ciborium as '=0.2.3', the differential oracles as '=0.2.2'",)
	"""
	return tuple(
		f"fuzz: fuzz/Cargo.toml depends on {crate} as {spec!r}, the differential oracles as {oracles[crate]!r}"
		for crate, spec in fuzz.items()
		if crate in oracles and spec != oracles[crate] and not (isinstance(spec, dict) and "path" in spec)
	)


def drifted_locks(workspace: Lock, fuzz: Lock) -> tuple[str, ...]:
	"""
	>>> from harness.differential import Package
	>>> drifted_locks(
	...     Lock(package=(Package(name="half", version="2.7.1"), Package(name="cddl", version="1"))),
	...     Lock(package=(Package(name="half", version="2.7.0"), Package(name="arbitrary", version="1"))),
	... )
	("fuzz: fuzz/Cargo.lock holds half at ['2.7.0'], Cargo.lock at ['2.7.1']",)
	"""
	return tuple(
		f"fuzz: fuzz/Cargo.lock holds {name} at {versions(fuzz, name)}, Cargo.lock at {versions(workspace, name)}"
		for name in sorted(
			{package.name for package in fuzz.package} & {package.name for package in workspace.package}
		)
		if versions(fuzz, name) != versions(workspace, name)
	)


def versions(lock: Lock, name: str) -> list[str]:
	return sorted(package.version for package in lock.package if package.name == name)


def resolved(source: str) -> Path:
	"""A seed source: a flake input's variable, then a path under it; or the fixtures' seeds of a shape.

	>>> resolved("fixtures/flat") == FIXTURES / "seeds" / "flat"
	True
	>>> resolved("RFC_VECTORS/rfc9529") == Path(os.environ["RFC_VECTORS"]) / "rfc9529"
	True
	"""
	variable, *under = source.split("/", 1)
	return (FIXTURES / "seeds" if variable == GENERATED else Path(os.environ[variable])).joinpath(*under)


def write_seeds() -> None:
	"""The fixtures crate's messages, each variant encoded, written by its own binary."""
	write_crate(FIXTURES, fixture_files())
	subprocess.run(
		["cargo", "run", "--quiet", "--bin", "seeds", "--", str(FIXTURES / "seeds")],
		cwd=FIXTURES,
		env={**os.environ, "CARGO_TARGET_DIR": str(HARNESS / "fuzz-fixtures")},
		check=True,
	)


def fixture_files() -> dict[str, str]:
	return {
		"Cargo.lock": (ROOT / "Cargo.lock").read_text(),
		"Cargo.toml": render(
			"Cargo.toml",
			name="fuzz-fixtures",
			dependencies="\n".join(
				(
					dependencies((HEAPLESS, "alloc", "serde", "std")),
					serde_dependency("std"),
					serde_bytes_dependency("std"),
				)
			),
		),
		"src/lib.rs": "#![allow(dead_code)]\n\nuse serde::{Deserialize, Serialize};\n\n"
		+ "".join(f"pub mod {shape} {{\npub mod both {{\n{both(shape)}\n}}\n}}\n\n" for shape in SHAPES)
		+ f"pub mod forms {{\nuse super::{{Deserialize, Serialize}};\n\n{render('serde_forms.rs')}}}\n\n"
		+ "".join(each(shape) for shape in SHAPES),
		"src/bin/seeds.rs": seeds_bin(),
	}


def each(shape: str) -> str:
	"""A macro that hands a callback macro an input and every message type of ``shape``."""
	types = ", ".join(
		f"$crate::{shape}::both::{m.name}{m.lifetime.replace('a', '_')}" for m in messages(COUNT, shape)
	)
	return (
		f"#[macro_export]\nmacro_rules! each_{shape} {{\n"
		f"    ($($callback:ident)::+, $input:expr) => {{ $($callback)::+!($input, [{types}]) }};\n}}\n\n"
	)


def seeds_bin() -> str:
	"""A binary that writes every variant of every message, one file each, under a directory per shape."""
	writers = "".join(
		f"fn {shape}_{m.name.lower()}(out: &Path) {{\n"
		+ "".join(
			f'    write(out, "{shape}", "{m.name}_{variant}", &{value(m, variant, f"fuzz_fixtures::{shape}::both")});\n'
			for variant in VARIANTS
		)
		+ "}\n\n"
		for shape in SHAPES
		for m in messages(COUNT, shape)
	)
	calls = "".join(
		f"    {shape}_{m.name.lower()}(&out);\n" for shape in SHAPES for m in messages(COUNT, shape)
	)
	return (
		"use std::path::Path;\n\n"
		"fn main() {\n"
		'    let out = std::path::PathBuf::from(std::env::args_os().nth(1).expect("a directory to write the seeds in"));\n'
		f"{calls}}}\n\n"
		"fn write<T: cbor_static::Encode>(out: &Path, shape: &str, name: &str, value: &T) {\n"
		'    std::fs::create_dir_all(out.join(shape)).expect("the seed directory is created");\n'
		'    std::fs::write(out.join(shape).join(format!("{name}.cbor")), cbor_static::to_vec(value).expect("a message encodes"))\n'
		'        .expect("the seed is written");\n'
		"}\n\n" + writers
	)


if __name__ == "__main__":
	app()
