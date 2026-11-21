from typing import TYPE_CHECKING, NamedTuple

from frames import Files, group, qualified
from pydantic import BaseModel, ConfigDict
from rust import EXTRA_ENTRIES

if TYPE_CHECKING:
	from collections.abc import Iterable

	from smp.message import _MessageBase


class Generated(NamedTuple):
	files: Files
	oracle: tuple[OracleCase, ...]


def oracle_files(cases: Iterable[OracleCase], smp_rev: str) -> Files:
	"""The manifest of the twins the smp oracle reads, the generated list of the Rust types they
	decode as, and the JPHutchins/smp revision that wrote them.
	"""
	manifest = OracleManifest(cases=tuple({case.name: case for case in cases}.values()))
	return {
		"oracle.json": manifest.model_dump_json(indent=1).encode(),
		"smp.rev": smp_rev.encode(),
		"oracle_types.rs": "".join(
			[
				"oracle_types! {\n",
				*(f'    ("{rust}", {rust}),\n' for rust in sorted({case.rust for case in manifest.cases})),
				"}\n",
			]
		).encode(),
	}


def oracle_case(name: str, cls: type[_MessageBase], module: str, *, firmware: bool) -> OracleCase:
	return OracleCase(
		name=name,
		rust=rust_path(cls, module),
		smp=qualified(cls),
		entries=qualified(entry) if (entry := EXTRA_ENTRIES.get(cls)) is not None else None,
		firmware=firmware,
	)


def rust_path(cls: type[_MessageBase], module: str) -> str:
	"""
	The Rust type of an smp class, by its path from `cbor_static_conformance::smp`.

	>>> from smp.image_management import ImageSlotInfoReadRequest
	>>> from smp.os_management import EchoWriteRequest, OSManagementErrorV2, ResetWriteRequest
	>>> [rust_path(cls, "") for cls in (OSManagementErrorV2, EchoWriteRequest, ImageSlotInfoReadRequest)]
	['ErrorV2', 'os::EchoWriteRequest', 'image::ImageSlotInfoReadRequest']
	>>> from rust import SMP_ONLY
	>>> rust_path(ResetWriteRequest, SMP_ONLY)
	'smp_only::ResetWriteRequest'
	"""
	match module, cls.__name__.removeprefix("OSManagement"):
		case "", ("ErrorV1" | "ErrorV2") as error:
			return error
		case "", name:
			return f"{group(cls)}::{name}"
		case _, name:
			return f"{module}{name}"


class OracleCase(BaseModel):
	"""A twin, the Rust type it decodes as, the smp class smp reads it as, and the class of that
	class's extra entries, if it has them. A firmware case's whole frame is beside its twin.
	"""

	model_config = ConfigDict(extra="forbid", frozen=True)
	name: str
	rust: str
	smp: str
	entries: str | None
	firmware: bool


class OracleManifest(BaseModel):
	model_config = ConfigDict(extra="forbid", frozen=True)
	cases: tuple[OracleCase, ...]
