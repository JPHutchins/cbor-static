"""Informational serde baselines: minicbor-serde (CBOR) and postcard (not CBOR)."""

from typing import Final

from harness.backends.backend import (
	SERDE,
	SERDE_BYTES,
	Backend,
	flake_input,
	no_impls,
	path_dependency,
	roundtrip_fn,
	serde_map_field_attrs,
)
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS
from harness.model import EnumDef, Field, FieldKind

DERIVE: Final = "#[derive(serde::Serialize, serde::Deserialize)]"


def enum_def(enum: EnumDef) -> str:
	return f"{DERIVE}\npub enum {enum.name} {{ A, B(u32), C {{ x: u32, y: bool }} }}\n"


def positional_field_attrs(field: Field, _position: int) -> str:
	match field.kind:
		case FieldKind.BYTES:
			return '#[serde(borrow, with = "serde_bytes")] '
		case FieldKind.NESTED if field.borrows:
			return "#[serde(borrow)] "
		case (
			FieldKind.U32
			| FieldKind.U64
			| FieldKind.I32
			| FieldKind.BOOL
			| FieldKind.OPT_U32
			| FieldKind.STR
			| FieldKind.NESTED
			| FieldKind.ENUM
		):
			return ""


MINICBOR_SERDE: Final = Backend(
	name="minicbor-serde",
	dependencies=lambda: "\n".join(
		(
			MINICBOR_HAND_TEXTKEYS.dependencies(),
			path_dependency("minicbor-serde", flake_input("MINICBOR") / "minicbor-serde"),
			SERDE,
			SERDE_BYTES,
		)
	),
	prelude="",
	struct_attrs=DERIVE,
	field_attrs=serde_map_field_attrs,
	enum_def=enum_def,
	impls=no_impls,
	roundtrip=roundtrip_fn(
		"minicbor_serde::from_slice::<{T}>(input)", "minicbor_serde::to_slice(&m, out).ok()"
	),
)

POSTCARD: Final = Backend(
	name="postcard",
	dependencies=lambda: "\n".join(
		(path_dependency("postcard", flake_input("POSTCARD") / "source" / "postcard"), SERDE, SERDE_BYTES)
	),
	prelude="",
	struct_attrs=DERIVE,
	field_attrs=positional_field_attrs,
	enum_def=enum_def,
	impls=no_impls,
	roundtrip=roundtrip_fn(
		"postcard::from_bytes::<{T}>(input)", "postcard::to_slice(&m, out).ok().map(|s| s.len())"
	),
)
