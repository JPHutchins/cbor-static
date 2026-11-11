"""cbor-static under test: default glue enums (gated), repr tables and the one-shot engine (informational)."""

from typing import TYPE_CHECKING, Final

from harness.backends.backend import ROOT, Backend, no_impls, path_dependency
from harness.model import EnumDef, Field, FieldKind, Message
from harness.templates import render

if TYPE_CHECKING:
	from collections.abc import Callable

DERIVE: Final = "#[derive(cbor_static::Cbor)]"


def backend(name: str, repr_attr: str = "", rustflags: tuple[str, ...] = ()) -> Backend:
	return Backend(
		name=name,
		dependencies=path_dependency("cbor-static", ROOT / "cbor-static"),
		prelude=render("cbor_static_prelude.rs"),
		struct_attrs=DERIVE,
		field_attrs=field_attrs,
		enum_def=enum_def(repr_attr),
		impls=no_impls,
		roundtrip=roundtrip,
		rustflags=rustflags,
	)


def field_attrs(field: Field, _position: int) -> str:
	match field.kind:
		case FieldKind.OPT_U32:
			return f'#[serde(rename = "{field.key}", default, skip_serializing_if = "Option::is_none")] '
		case (
			FieldKind.U32
			| FieldKind.U64
			| FieldKind.I32
			| FieldKind.BOOL
			| FieldKind.STR
			| FieldKind.BYTES
			| FieldKind.NESTED
			| FieldKind.ENUM
		):
			return f'#[serde(rename = "{field.key}")] '


def enum_def(repr_attr: str) -> Callable[[EnumDef], str]:
	return lambda enum: (
		f"{DERIVE}{repr_attr}\npub enum {enum.name} {{ A, B(u32), C {{ x: u32, y: bool }} }}\n"
	)


def roundtrip(msg: Message) -> str:
	return (
		f"fn {msg.fn_name}(input: &[u8], out: &mut [u8]) -> usize {{ roundtrip::<{msg.name}>(input, out) }}\n"
	)


CBOR_STATIC: Final = backend("cbor-static")
CBOR_STATIC_REPR: Final = backend("cbor-static-repr", repr_attr="\n#[repr(C, u8)]")
CBOR_STATIC_ONESHOT: Final = backend("cbor-static-oneshot", rustflags=("--cfg", "cbor_static_oneshot"))
