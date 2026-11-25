"""The Cargo manifests camas tasks read, as typed models: the fuzz crate's targets and seeds, and the
workspace's lints.
"""

import tomllib
from typing import Final, Literal
lazy from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field

type Level = Literal["allow", "warn", "deny", "forbid"]


class Seeds(BaseModel):
	model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)
	cargo_fuzz: bool = Field(alias="cargo-fuzz")
	seeds: dict[str, tuple[str, ...]]


class Metadata(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	metadata: Seeds


class Target(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	name: str


class FuzzManifest(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)
	package: Metadata
	dependencies: dict[str, str | dict[str, object]]
	targets: tuple[Target, ...] = Field(alias="bin")

	@classmethod
	def read(cls, root: Path) -> FuzzManifest:
		return cls.model_validate(tomllib.loads((root / "fuzz" / "Cargo.toml").read_text()))


class Lint(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	level: Level
	priority: int = 0


class Lints(BaseModel):
	model_config = ConfigDict(extra="forbid", frozen=True)
	rust: dict[str, Level | Lint]
	clippy: dict[str, Level | Lint]

	def flags(self) -> str:
		"""Rustc's command-line form of these lints, lowest priority first, so later flags win.

		>>> Lints(rust={"unsafe_code": "deny"}, clippy={"pedantic": Lint(level="warn", priority=-1)}).flags()
		'-W clippy::pedantic -D unsafe-code'
		"""
		return " ".join(
			f"{FLAGS[lint.level]} {name}"
			for name, lint in sorted(
				(
					(
						f"{prefix}{name.replace('_', '-')}",
						setting if isinstance(setting, Lint) else Lint(level=setting),
					)
					for prefix, table in (("", self.rust), ("clippy::", self.clippy))
					for name, setting in table.items()
				),
				key=lambda named: (named[1].priority, named[0]),
			)
		)


FLAGS: Final[dict[Level, str]] = {"allow": "-A", "warn": "-W", "deny": "-D", "forbid": "-F"}


class Workspace(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	lints: Lints


class RootManifest(BaseModel):
	model_config = ConfigDict(extra="allow", frozen=True)
	workspace: Workspace

	@classmethod
	def read(cls, root: Path) -> RootManifest:
		return cls.model_validate(tomllib.loads((root / "Cargo.toml").read_text()))
