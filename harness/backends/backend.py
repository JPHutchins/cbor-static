"""The shape every backend fills in, and helpers shared by them."""

import os
from pathlib import Path
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.model import EnumDef, Field, FieldKind, Message
from harness.templates import render

if TYPE_CHECKING:
	from collections.abc import Callable

ROOT: Final = Path(__file__).resolve().parents[2]
SERDE: Final = 'serde = { version = "=1.0.229", default-features = false, features = ["derive"] }'
SERDE_BYTES: Final = 'serde_bytes = { version = "=0.11.19", default-features = false }'


class Backend(NamedTuple):
	name: str
	dependencies: Callable[[], str]
	prelude: str
	struct_attrs: str
	field_attrs: Callable[[Field, int], str]
	enum_def: Callable[[EnumDef], str]
	impls: Callable[[Message], str]
	roundtrip: Callable[[Message], str]
	rustflags: tuple[str, ...] = ()


def rust_type(field: Field) -> str:
	"""
	>>> rust_type(Field("buf", FieldKind.BYTES)), rust_type(Field("err", FieldKind.OPT_U32))
	("&'a [u8]", 'Option<u32>')

	Raises:
		ValueError: If a nested or enum field lacks its target.
	"""
	match (field.kind, field.target):
		case (FieldKind.NESTED, Message() as nested):
			return f"{nested.name}{nested.lifetime}"
		case (FieldKind.ENUM, EnumDef(name)):
			return name
		case (FieldKind.U32, _):
			return "u32"
		case (FieldKind.U64, _):
			return "u64"
		case (FieldKind.I32, _):
			return "i32"
		case (FieldKind.BOOL, _):
			return "bool"
		case (FieldKind.OPT_U32, _):
			return "Option<u32>"
		case (FieldKind.STR, _):
			return "&'a str"
		case (FieldKind.BYTES, _):
			return "&'a [u8]"
		case (FieldKind.NESTED | FieldKind.ENUM, _):
			raise ValueError(f"{field} lacks its target")


def flake_input(variable: str) -> Path:
	"""
	>>> flake_input("MINICBOR").joinpath("minicbor", "Cargo.toml").is_file()
	True
	>>> flake_input("POSTCARD").joinpath("source", "postcard", "Cargo.toml").is_file()
	True
	"""
	return Path(os.environ[variable])


def no_impls(_msg: Message) -> str:
	return ""


def path_dependency(name: str, path: Path) -> str:
	"""
	>>> path_dependency("minicbor", Path("/x/minicbor"))
	'minicbor = { path = "/x/minicbor", default-features = false }'
	"""
	return f'{name} = {{ path = "{path}", default-features = false }}'


def roundtrip_fn(decode: str, encode: str) -> Callable[[Message], str]:
	return lambda msg: render(
		"roundtrip.rs", fn_name=msg.fn_name, decode=decode.format(T=msg.name), encode=encode
	)


def serde_map_field_attrs(field: Field, _position: int) -> str:
	match field.kind:
		case FieldKind.BYTES:
			return f'#[serde(rename = "{field.key}", borrow, with = "serde_bytes")] '
		case FieldKind.OPT_U32:
			return f'#[serde(rename = "{field.key}", default, skip_serializing_if = "Option::is_none")] '
		case FieldKind.NESTED if field.borrows:
			return f'#[serde(rename = "{field.key}", borrow)] '
		case (
			FieldKind.U32
			| FieldKind.U64
			| FieldKind.I32
			| FieldKind.BOOL
			| FieldKind.STR
			| FieldKind.NESTED
			| FieldKind.ENUM
		):
			return f'#[serde(rename = "{field.key}")] '
