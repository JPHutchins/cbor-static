"""JPHutchins/smp's deserializer as an oracle for cbor-static's SMP encodings: smp reads a payload
exactly when cbor-static does, except a shape pinned to the issue that records why one of them
refuses it, which must still be refused for that reason.
"""

import dataclasses
import importlib
import importlib.metadata
import shlex
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING, Annotated, Final

import cbor2
from cyclopts import App, Parameter
from pydantic import BaseModel, ConfigDict, ValidationError
from smp import header as smpheader
from smp.exceptions import SMPException
from smp.message import _MessageBase

if TYPE_CHECKING:
	from collections.abc import Iterator


ORACLE_FILES: Final = ("oracle.json", "oracle_types.rs", "smp.rev")


app = App(help=__doc__)


@app.default
def check(
	example: Annotated[str, Parameter(help="The `cargo run` command of the example that prints the lines.")],
	vectors: Annotated[
		Path,
		Parameter(env_var="SMP_VECTORS", help="The vectors, which name the smp revision that wrote them."),
	],
) -> None:
	"""
	Raises:
		SystemExit: the vectors' smp revision is not this smp's, or smp and cbor-static disagree on a
			line other than as its pin says.
	"""
	if missing := [name for name in ORACLE_FILES if not (vectors / name).is_file()]:
		raise SystemExit(
			f"smp oracle: SMP_VECTORS ({vectors}) has no {missing}, so it predates this checkout's "
			"nix/smp-vectors.nix; re-enter `nix develop`"
		)
	if (wrote := (vectors / "smp.rev").read_text()) != (
		reads := DirectUrl.model_validate_json(
			importlib.metadata.distribution("smp").read_text("direct_url.json") or "{}"
		).vcs_info.commit_id
	):
		raise SystemExit(f"smp oracle: smp at {wrote} wrote the vectors, and smp at {reads} reads them")
	lines = [
		Line.model_validate_json(line)
		for line in subprocess.run(
			shlex.split(example), stdout=subprocess.PIPE, text=True, check=True
		).stdout.splitlines()
	]
	if unexpected := [
		f"{line.name}: {surprise}" for line in lines if (surprise := verdict(line, unread_by_smp(line)))
	]:
		raise SystemExit("\n".join([f"smp oracle: {len(unexpected)} of {len(lines)} lines", *unexpected]))
	print(f"smp oracle: {len(lines)} lines, {sum(line.diverges is not None for line in lines)} pinned")


def verdict(line: Line, unread: str | None) -> str | None:
	"""
	What is unexpected about smp's reading of a line, if anything.

	>>> echo = Line(name="e", smp="s", entries=None, diverges=None, header=None, payload="a0", rejected_by_cbor_static=None)
	>>> rejected = echo.model_copy(update={"rejected_by_cbor_static": "MissingField"})
	>>> verdict(echo, None) is None, verdict(rejected, "smp too") is None
	(True, True)
	>>> verdict(echo, "ValidationError")
	'smp does not read what cbor-static reads (header None, payload a0): ValidationError'
	>>> pinned = echo.model_copy(update={"diverges": Pin(issue="JPHutchins/smp#79", reason="smp writes")})
	>>> verdict(pinned, "smp writes a165666f72636501") is None
	True
	>>> verdict(pinned, None)
	'smp and cbor-static agree on the shape JPHutchins/smp#79 pins (header None, payload a0)'
	>>> verdict(pinned, "CBORDecodeError")
	'the shape JPHutchins/smp#79 pins is refused for another reason (header None, payload a0): CBORDecodeError'
	"""
	bytes_of = f"(header {line.header}, payload {line.payload})"
	refusal = refusal_of_one(unread, line.rejected_by_cbor_static)
	match line.diverges:
		case None if refusal is None:
			return None
		case None:
			return (
				f"smp reads what cbor-static rejects as {line.rejected_by_cbor_static} {bytes_of}"
				if unread is None
				else f"smp does not read what cbor-static reads {bytes_of}: {unread}"
			)
		case Pin(issue=issue, reason=reason):
			if refusal is None:
				return f"smp and cbor-static agree on the shape {issue} pins {bytes_of}"
			return (
				None
				if reason in refusal
				else f"the shape {issue} pins is refused for another reason {bytes_of}: {refusal}"
			)


def refusal_of_one(unread: str | None, rejected: str | None) -> str | None:
	"""
	Why one reader refuses a payload, when exactly one of smp and cbor-static does.

	>>> refusal_of_one("smp", None), refusal_of_one(None, "cbor-static"), refusal_of_one("smp", "cbor-static")
	('smp', 'cbor-static', None)
	"""
	match unread, rejected:
		case str(), None:
			return unread
		case None, str():
			return rejected
		case _:
			return None


def unread_by_smp(line: Line) -> str | None:
	"""Why smp does not read the payload as written, or None when it does: smp loads the decoded map
	under the header, writes it back as cbor2's canonical form of the payload, and keeps nothing
	unread as an extra field, apart from a class's named entries, which it reads as their class.
	"""
	cls = smp_class(line.smp, _MessageBase)
	data = cbor2.loads(bytes.fromhex(line.payload))
	canonical = cbor2.dumps(data, canonical=True)
	header = header_of(cls, line.header, len(canonical))
	try:
		message = cls.load(header, {**data, "version": header.version})
	except (ValidationError, SMPException) as error:
		return f"{type(error).__name__}: {error}"
	if (written := bytes(message)[smpheader.Header.SIZE :]) != canonical:
		return f"smp writes {written.hex()}"
	if extras := list(unread(message, None if line.entries is None else smp_class(line.entries, BaseModel))):
		return f"smp keeps {extras} unread"
	return None


def unread(value: object, entries: type[BaseModel] | None = None) -> Iterator[str]:
	"""
	The extra fields of a value's models, at any depth, but a top level's that read as its named
	`entries`.

	>>> class Pool(BaseModel, extra="forbid"):
	...     nfree: int
	>>> class Inner(BaseModel, extra="allow"):
	...     known: int
	>>> class Outer(BaseModel, extra="allow"):
	...     inner: list[Inner]
	>>> list(unread(Outer.model_validate({"inner": [{"known": 1, "mpools": {}}], "pool": {}})))
	['pool', 'mpools']
	>>> list(unread(Outer.model_validate({"inner": [], "pool": {"nfree": 1}, "mpools": {"0": {}}}), Pool))
	['mpools']
	"""
	match value:
		case BaseModel():
			yield from (key for key, item in (value.model_extra or {}).items() if not reads_as(entries, item))
			yield from (found for _, item in value for found in unread(item))
		case list() | tuple():
			yield from (found for item in value for found in unread(item))
		case dict():
			yield from (found for item in value.values() for found in unread(item))
		case _:
			pass


def reads_as(entries: type[BaseModel] | None, item: object) -> bool:
	if entries is None:
		return False
	try:
		entries.model_validate(item)
	except ValidationError:
		return False
	return True


def header_of(cls: type[_MessageBase], firmware: str | None, length: int) -> smpheader.Header:
	"""
	The firmware's header with the canonical payload's length, or the one smp writes for `cls`.

	>>> from smp.os_management import OSManagementErrorV1
	>>> header_of(OSManagementErrorV1, None, 3)
	Traceback (most recent call last):
	ValueError: OSManagementErrorV1 has no header of its own, so its line must carry the firmware's

	Raises:
		ValueError: `cls` is an error class, which writes no header of its own, and there is no
			firmware header.
	"""
	match firmware:
		case str():
			return dataclasses.replace(smpheader.Header.loads(bytes.fromhex(firmware)), length=length)
		case None if hasattr(cls, "_OP"):
			return smpheader.Header(
				op=cls._OP,
				version=smpheader.Version.V2,
				flags=smpheader.Flag(cls._FLAGS),
				length=length,
				group_id=cls._GROUP_ID,
				sequence=0,
				command_id=cls._COMMAND_ID,
			)
		case None:
			raise ValueError(
				f"{cls.__name__} has no header of its own, so its line must carry the firmware's"
			)


def smp_class[T](name: str, base: type[T]) -> type[T]:
	"""
	>>> smp_class("smp.os_management.EchoWriteRequest", BaseModel).__name__
	'EchoWriteRequest'

	Raises:
		TypeError: the name is not a subclass of `base`.
	"""
	module, _, cls = name.rpartition(".")
	found: object = getattr(importlib.import_module(module), cls)
	if not (isinstance(found, type) and issubclass(found, base)):
		raise TypeError(f"{name} is not a {base.__name__}")
	return found


class DirectUrl(BaseModel):
	"""Where the installed smp came from (PEP 610's direct_url.json), as far as the oracle reads it."""

	vcs_info: VcsInfo


class VcsInfo(BaseModel):
	commit_id: str


class Pin(BaseModel):
	"""The issue that records a reader refusing a shape, and what its refusal says."""

	model_config = ConfigDict(extra="forbid", frozen=True)
	issue: str
	reason: str


class Line(BaseModel):
	"""One line the example prints, as its `Line` writes it."""

	model_config = ConfigDict(extra="forbid", frozen=True)
	name: str
	smp: str
	entries: str | None
	diverges: Pin | None
	header: str | None
	payload: str
	rejected_by_cbor_static: str | None


if __name__ == "__main__":
	app()
