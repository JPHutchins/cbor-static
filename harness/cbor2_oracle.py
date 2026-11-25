"""Python cbor2 as a differential oracle: reads one request per line, each an item of CBOR, and
answers each with cbor2's re-encoding of what it decodes, or why it refuses the item.

Every semantic tag stays a raw `CBORTag`, so cbor2 compares the data model rather than its own
datetimes, bignums, regular expressions and shared references.
"""

import sys
from typing import TYPE_CHECKING, Annotated, Final, Literal

import cbor2
from cyclopts import App
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

if TYPE_CHECKING:
	from collections.abc import Iterable

HEX: Final = ConfigDict(ser_json_bytes="hex", val_json_bytes="hex", extra="forbid", frozen=True)
SELF_DESCRIBED: Final = 55799
TAGS: Final = (*range(2**16), SELF_DESCRIBED)


class Request(BaseModel):
	model_config = HEX
	cbor: bytes


class Reencoded(BaseModel):
	model_config = HEX
	kind: Literal["reencoded"] = "reencoded"
	cbor: bytes


class Refused(BaseModel):
	model_config = HEX
	kind: Literal["refused"] = "refused"
	reason: str


REPLY: Final = TypeAdapter[Reencoded | Refused](Annotated[Reencoded | Refused, Field(discriminator="kind")])


def raw(tag: int) -> cbor2.SemanticDecoderCallback:
	return lambda value, _immutable: cbor2.CBORTag(tag, value)


RAW_TAGS: Final = {tag: raw(tag) for tag in TAGS}


def reply(item: bytes) -> Reencoded | Refused:
	"""
	cbor2's re-encoding of `item`, decoded with every tag kept raw.

	>>> reply(bytes.fromhex("82c0617a05")).cbor.hex()
	'82c0617a05'
	>>> reply(bytes.fromhex("c24101")).cbor.hex()
	'c24101'
	>>> reply(bytes.fromhex("bf61610161620203ff")).kind
	'refused'
	>>> reply(bytes.fromhex("a20100f500")).cbor.hex()
	'a10100'
	"""
	try:
		return Reencoded(cbor=cbor2.dumps(cbor2.loads(item, semantic_decoders=RAW_TAGS)))
	except cbor2.CBORError as error:
		return Refused(reason=f"{type(error).__name__}: {error}")


def replies(lines: Iterable[str]) -> Iterable[str]:
	r"""
	>>> list(replies(['{"cbor": "f97c01"}\n', '{"cbor": "1c"}\n']))
	['{"kind":"reencoded","cbor":"f97e00"}', '{"kind":"refused","reason":"CBORDecodeError: ...
	"""
	return (REPLY.dump_json(reply(Request.model_validate_json(line).cbor)).decode() for line in lines)


app = App(help=__doc__)


@app.default
def serve() -> None:
	for answer in replies(sys.stdin):
		print(answer, flush=True)


if __name__ == "__main__":
	app()
