"""Message shapes shared by the generator, the backends and the value generator."""

from enum import Enum, auto
from typing import Final, NamedTuple


class FieldKind(Enum):
	U32 = auto()
	U64 = auto()
	I32 = auto()
	BOOL = auto()
	OPT_U32 = auto()
	STR = auto()
	BYTES = auto()
	NESTED = auto()
	ENUM = auto()


FLAT_KINDS: Final = (
	FieldKind.U32,
	FieldKind.U64,
	FieldKind.I32,
	FieldKind.BOOL,
	FieldKind.OPT_U32,
	FieldKind.STR,
	FieldKind.BYTES,
)
RICH_KINDS: Final = (*FLAT_KINDS, FieldKind.NESTED, FieldKind.ENUM)
SHAPES: Final = {"flat": FLAT_KINDS, "rich": RICH_KINDS}


class EnumDef(NamedTuple):
	"""Always ``{ A, B(u32), C { x: u32, y: bool } }``: one unit, one newtype and one struct variant."""

	name: str


class Field(NamedTuple):
	key: str
	kind: FieldKind
	target: Message | EnumDef | None = None

	@property
	def ident(self) -> str:
		"""
		>>> Field("off", FieldKind.U32).ident
		'f_off'
		"""
		return f"f_{self.key}"

	@property
	def borrows(self) -> bool:
		"""
		>>> Field("data", FieldKind.BYTES).borrows, Field("off", FieldKind.U32).borrows
		(True, False)
		>>> Field("hdr", FieldKind.NESTED, Message("Msg0", (Field("rsn", FieldKind.STR),))).borrows
		True
		"""
		match self.target:
			case Message() as nested:
				return nested.borrows
			case EnumDef() | None:
				return self.kind in (FieldKind.STR, FieldKind.BYTES)


class Message(NamedTuple):
	name: str
	fields: tuple[Field, ...]

	@property
	def borrows(self) -> bool:
		return any(field.borrows for field in self.fields)

	@property
	def lifetime(self) -> str:
		return "<'a>" if self.borrows else ""

	@property
	def fn_name(self) -> str:
		return f"roundtrip_{self.name.lower()}"

	@property
	def enums(self) -> tuple[EnumDef, ...]:
		return tuple(field.target for field in self.fields if isinstance(field.target, EnumDef))
