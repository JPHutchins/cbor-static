"""Deterministic message types and the no_std thumbv7m crates that round-trip them per backend."""

import random
from functools import reduce
from typing import TYPE_CHECKING, Final

from harness.backends import rust_type
from harness.model import SHAPES, EnumDef, Field, FieldKind, Message
from harness.templates import render

if TYPE_CHECKING:
	from harness.backends.backend import Backend

KEY_POOL: Final = (
	"off",
	"len",
	"data",
	"sha",
	"upgrade",
	"image",
	"slot",
	"rc",
	"rsn",
	"version",
	"hash",
	"bootable",
	"pending",
	"confirmed",
	"active",
	"permanent",
	"name",
	"mtu",
	"count",
	"seq",
	"group",
	"id",
	"flags",
	"ts",
	"value",
	"status",
	"msg",
	"err",
	"buf",
	"size",
)


def render_crate(backend: Backend, count: int, shape: str) -> dict[str, str]:
	generated = messages(count, shape)
	return {
		"Cargo.toml": render(
			"Cargo.toml", name=crate_name(shape, backend, count), dependencies=backend.dependencies()
		),
		"src/main.rs": render(
			"main.rs",
			prelude=backend.prelude,
			items="\n".join(
				render_message(backend, m) + "\n" + backend.impls(m) + "\n" + backend.roundtrip(m)
				for m in generated
			),
			calls="\n".join(f"    black_box({m.fn_name}(input, out));" for m in generated),
		),
	}


def crate_name(shape: str, backend: Backend, count: int) -> str:
	return f"cs-{shape}-{backend.name}-n{count}"


def messages(count: int, shape: str) -> tuple[Message, ...]:
	"""The first ``count`` messages of ``shape``, seeded per index so prefixes are stable.

	>>> [m.name for m in messages(3, "flat")]
	['Msg0', 'Msg1', 'Msg2']
	>>> [(f.key, f.kind.name) for f in messages(1, "flat")[0].fields]
	[('value', 'BOOL'), ('buf', 'BOOL'), ('confirmed', 'BYTES'), ('len', 'BYTES'), ('rsn', 'I32'), ('name', 'BOOL')]
	"""
	kinds = SHAPES[shape]
	return reduce(lambda acc, index: (*acc, message(index, acc, kinds)), range(count), ())


def message(index: int, previous: tuple[Message, ...], kinds: tuple[FieldKind, ...]) -> Message:
	rng = random.Random(index)
	name = f"Msg{index}"
	return Message(
		name=name,
		fields=tuple(
			field(key, rng.choice(kinds), name, previous, rng)
			for key in rng.sample(KEY_POOL, rng.randint(3, 6))
		),
	)


def field(key: str, kind: FieldKind, owner: str, previous: tuple[Message, ...], rng: random.Random) -> Field:
	match kind:
		case FieldKind.NESTED if previous:
			return Field(key, kind, rng.choice(previous))
		case FieldKind.NESTED:
			return Field(key, FieldKind.U32)
		case FieldKind.ENUM:
			return Field(key, kind, EnumDef(f"{owner}{key.capitalize()}Kind"))
		case _:
			return Field(key, kind)


def render_message(backend: Backend, msg: Message) -> str:
	fields = "\n".join(
		f"    {backend.field_attrs(field, position)}pub {field.ident}: {rust_type(field)},"
		for position, field in enumerate(msg.fields)
	)
	return (
		"".join(map(backend.enum_def, msg.enums))
		+ f"{backend.struct_attrs}\npub struct {msg.name}{msg.lifetime} {{\n{fields}\n}}\n"
	)
