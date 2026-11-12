"""The gate baseline: hand-written minicbor impls with text keys, ported verbatim from the research harness."""

from typing import Final

from harness.backends.backend import Backend, flake_input, path_dependency, roundtrip_fn
from harness.model import EnumDef, Field, FieldKind, Message
from harness.templates import render


def impls(msg: Message) -> str:
	return render(
		"minicbor_hand_struct.rs",
		name=msg.name,
		lifetime=msg.lifetime,
		impl_lifetime="<'a>" if msg.borrows else "",
		decode_lifetime="'a" if msg.borrows else "'b",
		entries=str(sum(field.kind != FieldKind.OPT_U32 for field in msg.fields))
		+ "".join(f" + self.{f.ident}.is_some() as u64" for f in msg.fields if f.kind == FieldKind.OPT_U32),
		encode_fields="".join(map(encode_field, msg.fields)),
		lets="".join(f"        let mut {f.ident} = None;\n" for f in msg.fields),
		arms="".join(
			f'                "{f.key}" => {f.ident} = Some({decode_expr(f.kind)}),\n' for f in msg.fields
		),
		finish="".join(finish_field(f, i) for i, f in enumerate(msg.fields)),
	)


def encode_field(field: Field) -> str:
	match field.kind:
		case FieldKind.OPT_U32:
			return f'        if let Some(v) = self.{field.ident} {{ e.str("{field.key}")?.u32(v)?; }}\n'
		case FieldKind.NESTED | FieldKind.ENUM:
			return f'        e.str("{field.key}")?.encode(&self.{field.ident})?;\n'
		case FieldKind.U32 | FieldKind.U64 | FieldKind.I32 | FieldKind.BOOL | FieldKind.STR | FieldKind.BYTES:
			return f'        e.str("{field.key}")?.{scalar_method(field.kind)}(self.{field.ident})?;\n'


def decode_expr(kind: FieldKind) -> str:
	match kind:
		case FieldKind.NESTED | FieldKind.ENUM:
			return "d.decode()?"
		case (
			FieldKind.U32
			| FieldKind.U64
			| FieldKind.I32
			| FieldKind.BOOL
			| FieldKind.OPT_U32
			| FieldKind.STR
			| FieldKind.BYTES
		):
			return f"d.{scalar_method(kind)}()?"


def scalar_method(kind: FieldKind) -> str:
	match kind:
		case FieldKind.U32 | FieldKind.OPT_U32:
			return "u32"
		case FieldKind.U64:
			return "u64"
		case FieldKind.I32:
			return "i32"
		case FieldKind.BOOL:
			return "bool"
		case FieldKind.STR:
			return "str"
		case FieldKind.BYTES:
			return "bytes"
		case FieldKind.NESTED | FieldKind.ENUM:
			raise ValueError(f"{kind} is not a scalar")


def finish_field(field: Field, position: int) -> str:
	match field.kind:
		case FieldKind.OPT_U32:
			return f"            {field.ident},\n"
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
			return f"            {field.ident}: {field.ident}.ok_or_else(|| minicbor::decode::Error::missing_value({position}))?,\n"


def enum_def(enum: EnumDef) -> str:
	return render("minicbor_hand_enum.rs", name=enum.name)


MINICBOR_HAND_TEXTKEYS: Final = Backend(
	name="minicbor-hand-textkeys",
	dependencies=lambda: path_dependency("minicbor", flake_input("MINICBOR") / "minicbor"),
	prelude="",
	struct_attrs="",
	field_attrs=lambda _field, _position: "",
	enum_def=enum_def,
	impls=impls,
	roundtrip=roundtrip_fn(
		decode="minicbor::decode::<{T}>(input)",
		encode="{ let mut c = minicbor::encode::write::Cursor::new(out); minicbor::encode(&m, &mut c).ok().map(|()| c.position()) }",
	),
)
