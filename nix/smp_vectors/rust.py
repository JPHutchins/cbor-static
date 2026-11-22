import hashlib
from typing import TYPE_CHECKING, Final

from frames import MESSAGE_FIELDS, require, snake
from pydantic import BaseModel
from smp import image_management, os_management
from smp.message import _MessageBase

if TYPE_CHECKING:
	from collections.abc import Iterable, Iterator


def generated(macro: str, blocks: Iterable[tuple[str, str, Iterable[str]]]) -> str:
	"""A macro invocation with a block per command: its Rust type, and one line per case."""
	return "".join(
		[
			f"{macro}! {{\n",
			*(
				f"    {command}: {rust} {{\n" + "".join(f"{line}\n" for line in lines) + "    }\n"
				for command, rust, lines in blocks
			),
			"}\n",
		]
	)


def literal(value: object, module: str = "") -> str:
	match value:
		case bool():
			return str(value).lower()
		case int():
			return f"{int(value):_}"
		case str():
			require(value.isascii() and value.isprintable(), f"{value!r} is not printable ASCII")
			return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'
		case bytes() if len(value) <= INLINE_BYTES:
			return f'b"{"".join(f"\\x{byte:02x}" for byte in value)}".as_slice()'
		case bytes():
			return f'include_bytes!(concat!(env!("SMP_VECTORS"), "/{data_file(value)}")).as_slice()'
		case list() | tuple():
			return f"vec([{', '.join(literal(item, module) for item in value)}])"
		case dict():
			return f"map([{', '.join(f'({literal(key)}, {literal(item, module)})' for key, item in value.items())}])"
		case BaseModel() if (rendered := RENDERED_AS.get(type(value))) is not None:
			return model_literal(rendered.model_validate(dict(value)), module)
		case BaseModel():
			return model_literal(value, module)
		case _:
			raise TypeError(f"no Rust literal for {value!r}")


def model_literal(value: BaseModel, module: str) -> str:
	"""The Rust literal of a model, named as smp names its class. Its fields are those smp's
	message base does not add, and a field that may be absent, in smp's class or in the firmware's
	shape, is an `Option`, present when the model sets it. A field only the firmware writes is
	`None`. A model of named extra entries is a newtype over a map of them.
	"""
	name = f"{module}{type(value).__name__}"
	if (entry := EXTRA_ENTRIES.get(type(value))) is not None:
		extra = {key: entry.model_validate(item) for key, item in (value.model_extra or {}).items()}
		return f"{name}({literal(extra, module)})"
	optional = FIRMWARE_OPTIONAL.get(type(value), frozenset())
	fields = ", ".join(
		[
			*(
				f"{rust_field(field)}: {field_literal(value, field, info.is_required() and field not in optional, module)}"
				for field, info in type(value).model_fields.items()
				if not (isinstance(value, _MessageBase) and field in MESSAGE_FIELDS)
			),
			*(f"{rust_field(field)}: None" for field in FIRMWARE_ONLY_FIELDS.get(type(value), ())),
		]
	)
	return f"{name} {{ {fields} }}" if fields else f"{name} {{}}"


def field_literal(value: BaseModel, field: str, required: bool, module: str) -> str:
	item = getattr(value, field)
	if required:
		return literal(item, module)
	return "None" if item is None or field not in value.model_fields_set else f"Some({literal(item, module)})"


def long_bytes(value: object) -> Iterator[bytes]:
	"""The byte strings in a value that `literal` reads from a `data_file`."""
	match value:
		case bytes() if len(value) > INLINE_BYTES:
			yield value
		case list() | tuple():
			yield from (found for item in value for found in long_bytes(item))
		case dict():
			yield from (found for item in value.values() for found in long_bytes(item))
		case BaseModel():
			yield from long_bytes(
				{
					field: item
					for field, item in value
					if not (isinstance(value, _MessageBase) and field in MESSAGE_FIELDS)
				}
			)
		case _:
			pass


def data_file(value: bytes) -> str:
	return f"data/{hashlib.sha256(value).hexdigest()}.bin"


def rust_field(name: str) -> str:
	"""
	>>> rust_field("splitStatus"), rust_field("match")
	('split_status', 'r#match')
	"""
	field = snake(name)
	return f"r#{field}" if field in RUST_KEYWORDS else field


INLINE_BYTES: Final = 64


RUST_KEYWORDS: Final = frozenset(
	[
		"as",
		"async",
		"await",
		"break",
		"const",
		"continue",
		"crate",
		"dyn",
		"else",
		"enum",
		"extern",
		"false",
		"fn",
		"for",
		"gen",
		"if",
		"impl",
		"in",
		"let",
		"loop",
		"match",
		"mod",
		"move",
		"mut",
		"pub",
		"ref",
		"return",
		"self",
		"static",
		"struct",
		"super",
		"trait",
		"true",
		"type",
		"unsafe",
		"use",
		"where",
		"while",
	]
)


EXTRA_ENTRIES: Final[dict[type[BaseModel], type[BaseModel]]] = {
	os_management.MemoryPoolStatisticsReadResponse: os_management.MemoryPoolStatistics
}


RENDERED_AS: Final[dict[type[BaseModel], type[BaseModel]]] = {
	os_management.TaskStatistics: os_management.TaskStatisticsZephyr
}


FIRMWARE_OPTIONAL: Final[dict[type[BaseModel], frozenset[str]]] = {
	image_management.ImageSlot: frozenset(["size"])
}


FIRMWARE_ONLY_FIELDS: Final[dict[type[BaseModel], tuple[str, ...]]] = {
	image_management.ImageSlot: ("rc",),
	image_management.ImageEraseResponse: ("rc",),
}


SMP_ONLY: Final = "smp_only::"
