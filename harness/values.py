"""Deterministic Rust value literals for generated messages, covering every enum variant and Some/None."""

import random
import string
from typing import Final

from harness.model import EnumDef, Field, FieldKind, Message

U32S: Final = (0, 23, 24, 255, 256, 65_535, 65_536, 4_294_967_295)
U64S: Final = (*U32S, 4_294_967_296, 18_446_744_073_709_551_615)
I32S: Final = (0, -1, -24, -25, -256, 100, 2_147_483_647, -2_147_483_648)
STRING_LENGTHS: Final = (0, 1, 23, 24, 30)
BYTES_LENGTHS: Final = (0, 1, 24, 40)


def value(msg: Message, variant: int, module: str) -> str:
	"""Variant ``variant`` of ``msg`` as a Rust struct expression under ``module``.

	>>> kinds = Message("M", (Field("e", FieldKind.ENUM, EnumDef("K")), Field("o", FieldKind.OPT_U32)))
	>>> value(kinds, 0, "m")
	'm::M { f_e: m::K::A, f_o: None }'
	>>> value(kinds, 1, "m")
	'm::M { f_e: m::K::B(65536), f_o: Some(4294967295) }'
	"""
	rng = random.Random(f"{msg.name}-{variant}")
	fields = ", ".join(
		f"{field.ident}: {field_value(field, position + variant, variant, module, rng)}"
		for position, field in enumerate(msg.fields)
	)
	return f"{module}::{msg.name} {{ {fields} }}"


def field_value(field: Field, parity: int, variant: int, module: str, rng: random.Random) -> str:
	match (field.kind, field.target):
		case (FieldKind.NESTED, Message() as nested):
			return value(nested, variant, module)
		case (FieldKind.ENUM, EnumDef(name)):
			return (
				f"{module}::{name}::A",
				f"{module}::{name}::B({rng.choice(U32S)})",
				f"{module}::{name}::C {{ x: {rng.choice(U32S)}, y: {str(parity % 2 == 0).lower()} }}",
			)[variant % 3]
		case (FieldKind.U32, _):
			return str(rng.choice(U32S))
		case (FieldKind.U64, _):
			return str(rng.choice(U64S))
		case (FieldKind.I32, _):
			return str(rng.choice(I32S))
		case (FieldKind.BOOL, _):
			return str(parity % 2 == 0).lower()
		case (FieldKind.OPT_U32, _):
			return "None" if parity % 2 else f"Some({rng.choice(U32S)})"
		case (FieldKind.STR, _):
			return '"' + "".join(rng.choices(string.ascii_letters, k=rng.choice(STRING_LENGTHS))) + '"'
		case (FieldKind.BYTES, _):
			return "&[" + ", ".join(str(rng.randrange(256)) for _ in range(rng.choice(BYTES_LENGTHS))) + "]"
		case (FieldKind.NESTED | FieldKind.ENUM, _):
			raise ValueError(f"{field} lacks its target")
