"""The differential oracles over the harness's generated messages: for each message deriving both
cbor-static layers, ciborium, minicbor and cbor2 read what cbor-static writes as `Tokens` does, each
oracle's re-encoding reads back as the same bytes, and ciborium serializes the value to the bytes
cbor-static's serde layer writes. Hand-written serde forms check that last comparison too.
"""

import os
import subprocess
import tomllib
from typing import Final

from cyclopts import App
from pydantic import BaseModel, ConfigDict, Field

from harness.backends.backend import SERDE, SERDE_BYTES, flake_input
from harness.crosscheck import COUNT, VARIANTS, both
from harness.features import HEAPLESS, dependencies
from harness.generate import messages
from harness.measure import CRATES, HARNESS, write_crate
from harness.model import SHAPES
from harness.templates import render
from harness.values import value
from harness.workspace import ROOT

CONFORMANCE: Final = ROOT / "cbor-static-conformance"
COMMON: Final = CONFORMANCE / "tests" / "common" / "differential" / "mod.rs"
ORACLES: Final = ("ciborium", "minicbor", "serde_json")


class Dependency(BaseModel):
	model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)
	version: str
	default_features: bool = Field(default=True, alias="default-features")
	features: tuple[str, ...] = ()

	def line(self, crate: str) -> str:
		"""
		>>> Dependency(version="=2.3.1", default_features=False, features=("half",)).line("minicbor")
		'minicbor = { version = "=2.3.1", default-features = false, features = ["half"] }'
		>>> Dependency(version="=0.2.2").line("ciborium")
		'ciborium = { version = "=0.2.2", default-features = true, features = [] }'
		"""
		features = ", ".join(f'"{feature}"' for feature in self.features)
		return (
			f'{crate} = {{ version = "{self.version}", '
			f"default-features = {str(self.default_features).lower()}, features = [{features}] }}"
		)


class Manifest(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	dev_dependencies: dict[str, str | dict[str, object]] = Field(alias="dev-dependencies")

	def dependency(self, crate: str) -> Dependency:
		match self.dev_dependencies[crate]:
			case str(version):
				return Dependency(version=version)
			case spec:
				return Dependency.model_validate(spec)


class Package(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	name: str
	version: str


class Lock(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	package: tuple[Package, ...]

	def version(self, crate: str) -> str:
		"""
		>>> lock = Lock(package=(Package(name="half", version="2.7.1"), Package(name="rand", version="0.8.5"),
		...     Package(name="rand", version="0.10.3")))
		>>> lock.version("half")
		'2.7.1'
		>>> lock.version("rand")
		Traceback (most recent call last):
		SystemExit: differential: Cargo.lock holds rand at ['0.8.5', '0.10.3'], not one version
		>>> lock.version("minicbor")
		Traceback (most recent call last):
		SystemExit: differential: Cargo.lock holds minicbor at [], not one version

		Raises:
			SystemExit: the lock holds `crate` at no version, or at more than one.
		"""
		match [package.version for package in self.package if package.name == crate]:
			case [version]:
				return version
			case versions:
				raise SystemExit(f"differential: Cargo.lock holds {crate} at {versions}, not one version")


class CargoPackage(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	version: str


class CargoToml(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	package: CargoPackage


app = App(help=__doc__)


@app.default
def check() -> int:
	"""
	Raises:
		SystemExit: the minicbor oracle's version is not the flake's pinned minicbor.
	"""
	if drifted := drift(
		Lock.model_validate(tomllib.loads((ROOT / "Cargo.lock").read_text())).version("minicbor"),
		CargoToml.model_validate(
			tomllib.loads((flake_input("MINICBOR") / "minicbor" / "Cargo.toml").read_text())
		).package.version,
	):
		raise SystemExit(drifted)
	return subprocess.run(
		["cargo", "test", "--quiet"],
		cwd=write_crate(CRATES / "differential", files()),
		env={**os.environ, "CARGO_TARGET_DIR": str(HARNESS / "differential")},
		check=False,
	).returncode


def drift(locked: str, pinned: str) -> str | None:
	"""
	>>> drift("2.3.1", "2.3.1") is None
	True
	>>> drift("2.3.1", "2.4.0")
	"differential: the minicbor oracle is 2.3.1 in Cargo.lock, but the flake's MINICBOR is 2.4.0"
	"""
	return (
		None
		if locked == pinned
		else f"differential: the minicbor oracle is {locked} in Cargo.lock, but the flake's MINICBOR is {pinned}"
	)


def files() -> dict[str, str]:
	manifest = Manifest.model_validate(tomllib.loads((CONFORMANCE / "Cargo.toml").read_text()))
	return {
		"Cargo.lock": (ROOT / "Cargo.lock").read_text(),
		"Cargo.toml": render(
			"Cargo.toml",
			name="differential",
			dependencies="\n".join(
				(
					dependencies((HEAPLESS, "alloc", "serde", "std")),
					SERDE,
					SERDE_BYTES,
					"\n[dev-dependencies]",
					*(manifest.dependency(crate).line(crate) for crate in ORACLES),
				)
			),
		),
		"src/lib.rs": "#![allow(dead_code)]\n"
		+ "".join(f"pub mod {shape} {{\npub mod both {{\n{both(shape)}\n}}\n}}\n" for shape in SHAPES),
		"tests/common/mod.rs": render("differential_helpers.rs"),
		"tests/generated.rs": render(
			"differential_generated.rs", common=str(COMMON), tests="\n".join(map(cases, SHAPES))
		),
		"tests/serde_forms.rs": render("differential_serde_forms.rs", common=str(COMMON)),
	}


def cases(shape: str) -> str:
	return "\n".join(
		f"#[test]\nfn {shape}_{m.name.lower()}() {{\n"
		+ "".join(
			f"    differential!({shape}::both::{m.name}{m.lifetime.replace('a', '_')}, "
			f"{value(m, variant, f'{shape}::both')});\n"
			+ (
				""
				if m.borrows
				else f"    owned!({shape}::both::{m.name}, {value(m, variant, f'{shape}::both')});\n"
			)
			for variant in VARIANTS
		)
		+ "}\n"
		for m in messages(COUNT, shape)
	)


if __name__ == "__main__":
	app()
