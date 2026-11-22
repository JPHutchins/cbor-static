import json
from collections import Counter
from typing import TYPE_CHECKING, Final, NamedTuple

from frames import MESSAGE_FIELDS, TWIN, Files, group, message_class, payload_of, require, snake
from oracle import Generated, oracle_case
from pydantic import BaseModel, ConfigDict, JsonValue
from rust import SMP_ONLY, data_file, generated, literal, long_bytes
from smp import header as smpheader

if TYPE_CHECKING:
	from collections.abc import Callable, Iterable
	from pathlib import Path

	from smp.message import _MessageBase


def module_records(records: Path, module: str) -> Generated:
	"""Each record file's twins and generated cases, and its per-shape cases for the smp oracle."""
	files = [record_file(path) for path in record_files(records, module)]
	return Generated(
		{
			**{twin: payload for file in files for twin, payload in file.twins.items()},
			**{
				data_file(value): value
				for file in files
				for case in file.cases
				for value in long_bytes(case.message)
			},
			f"{module.removesuffix('_management')}_records.rs": generated(
				"records",
				(
					(
						snake(file.cls.__name__),
						f"{file.module}{file.cls.__name__}",
						map(Case.rust, file.cases),
					)
					for file in files
				),
			).encode(),
		},
		tuple(
			oracle_case(case.twin, file.cls, file.module, firmware=False)
			for file in files
			for case in file.cases
			if case.per_shape
		),
	)


def record_files(records: Path, module: str) -> list[Path]:
	files = sorted(records.glob(f"smp.{module}.*.json"))
	pinned = sorted(name for name in PINS if name.startswith(f"smp.{module}."))
	require(
		[path.name.removesuffix(".json") for path in files] == pinned,
		f"{module}: pinned {pinned}, found {files}",
	)
	return files


def record_file(path: Path) -> RecordFile:
	"""One case per distinct (payload, kwargs) input, named by the first line that holds it, and one
	twin per distinct payload, named likewise. A record of a shape only smp writes reads as the
	`smp_only` type. Miri runs the file's first line, one decode and encode path per type; the smp
	oracle reads the first input of each kwargs shape.
	"""
	name, lines = path.name.removesuffix(".json"), path.read_text().splitlines()
	records = {number: Record.model_validate_json(line) for number, line in enumerate(lines, start=1)}
	require(all(record.message == name for record in records.values()), f"{path.name}")
	messages = {number: validated(record) for number, record in records.items()}
	first_of_input = {
		(messages[number][1], json.dumps(records[number].kwargs, sort_keys=True)): number
		for number in reversed(records)
	}
	twin_of_payload = {messages[number][1]: number for number in reversed(records)}
	first_of_shape = first_of_each(first_of_input.values(), lambda number: shape(records[number].kwargs))
	found = Pins(
		dict(Counter(Counter(lines).values())), len(twin_of_payload), len(first_of_input), len(first_of_shape)
	)
	require(found == PINS[name], f"{path.name}: pinned {PINS[name]}, found {found}")
	directory, module = twin_directory(message_class(name)), SMP_ONLY if name in SMP_ONLY_RECORDS else ""
	return RecordFile(
		message_class(name),
		module,
		[
			Case(
				number,
				f"{directory}/{twin_of_payload[messages[number][1]]}",
				number == 1,
				number in frozenset(first_of_shape.values()),
				messages[number][0],
				module,
			)
			for number in sorted(first_of_input.values())
		],
		{f"{directory}/{number}{TWIN}": payload for payload, number in twin_of_payload.items()},
	)


class Case(NamedTuple):
	line: int
	twin: str
	miri: bool
	per_shape: bool
	message: _MessageBase
	module: str

	def rust(self) -> str:
		host_only = "" if self.miri else "#[cfg(not(miri))] "
		return f'        {host_only}(line_{self.line}, "{self.twin}", {literal(self.message, self.module)}),'


class RecordFile(NamedTuple):
	cls: type[_MessageBase]
	module: str
	cases: list[Case]
	twins: Files


def twin_directory(cls: type[_MessageBase]) -> str:
	return f"{group(cls)}/{snake(cls.__name__)}"


def validated(record: Record) -> tuple[_MessageBase, bytes]:
	"""The record's message, rebuilt by smp from its kwargs, and the frozen payload it agrees with."""
	cls, frame, kwargs = message_class(record.message), bytes.fromhex(record.bytes), kwargs_of(record)
	header = smpheader.Header.loads(frame[: smpheader.Header.SIZE])
	require((header.version, header.sequence) == (record.version, record.sequence), f"{header} for {record}")
	built = cls.model_validate({"version": record.version, "sequence": record.sequence, **kwargs})
	require(bytes(built) == frame, f"smp writes {bytes(built).hex()} for {record}")
	require(cls.loads(frame) == built, f"smp reads {record.bytes} differently from its kwargs")
	dumped = built.model_dump(exclude_unset=True, exclude_none=True, exclude=set(MESSAGE_FIELDS))
	require(dumped == kwargs, f"smp dumps {dumped} for {record}")
	return built, payload_of(cls, frame)


def kwargs_of(record: Record) -> dict[str, object]:
	"""The record's kwargs with their hex strings decoded, as test_binary_lock.py decodes `data`,
	`sha` and `val`. An image's `hash`, which that test overwrites with `b"A"`, is decoded from its hex
	instead, so the frozen bytes check it.
	"""
	return {key: decoded(key, value) for key, value in record.kwargs.items()}


def decoded(key: str, value: JsonValue) -> object:
	match key, value:
		case (("data" | "sha" | "val"), str()):
			return bytes.fromhex(value)
		case "images", list():
			return [
				{**image, "hash": bytes.fromhex(hash_hex)}
				if isinstance(image, dict) and isinstance(hash_hex := image.get("hash"), str)
				else image
				for image in value
			]
		case _:
			return value


def first_of_each(numbers: Iterable[int], key: Callable[[int], object]) -> dict[object, int]:
	"""
	The lowest number for each key.

	>>> first_of_each([2, 1, 3], lambda number: number % 2)
	{1: 1, 0: 2}
	"""
	return {key(number): number for number in sorted(numbers, reverse=True)}


def shape(value: object) -> object:
	match value:
		case dict():
			return tuple(sorted((key, shape(item)) for key, item in value.items()))
		case list():
			return tuple(map(shape, value))
		case _:
			return type(value).__name__


class Record(BaseModel):
	"""One line of a record file, as smp's test_binary_lock.py reads it."""

	model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
	message: str
	version: int
	sequence: int
	kwargs: dict[str, JsonValue]
	bytes: str


class Pins(NamedTuple):
	"""A record file as pinned: how many distinct lines occur how many times, and how many distinct
	payloads, (payload, kwargs) inputs and kwargs shapes it holds.
	"""

	repeats: dict[int, int]
	payloads: int
	inputs: int
	shapes: int


PINS: Final = {
	"smp.os_management.BootloaderInformationReadRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.BootloaderInformationReadResponse": Pins({1: 1}, 1, 1, 1),
	"smp.os_management.DateTimeReadRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.DateTimeReadResponse": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.DateTimeWriteRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.DateTimeWriteResponse": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.EchoWriteRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.EchoWriteResponse": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.MCUMgrParametersReadRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.MCUMgrParametersReadResponse": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.MCUbootModeReadRequest": Pins({1: 1}, 1, 1, 1),
	"smp.os_management.MCUbootModeReadResponse": Pins({1: 2}, 2, 2, 2),
	"smp.os_management.MemoryPoolStatisticsReadRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.MemoryPoolStatisticsReadResponse": Pins({1: 2}, 1, 1, 1),
	"smp.os_management.OSApplicationInfoReadRequest": Pins({2: 2}, 2, 2, 2),
	"smp.os_management.OSApplicationInfoReadResponse": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.ResetWriteRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.ResetWriteResponse": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.TaskStatisticsReadRequest": Pins({2: 1}, 1, 1, 1),
	"smp.os_management.TaskStatisticsReadResponse": Pins({2: 1}, 1, 1, 1),
	"smp.image_management.ImageEraseRequest": Pins({2: 1, 3: 1}, 2, 2, 2),
	"smp.image_management.ImageEraseResponse": Pins({3: 1}, 1, 1, 1),
	"smp.image_management.ImageStatesReadRequest": Pins({3: 1}, 1, 1, 1),
	"smp.image_management.ImageStatesReadResponse": Pins({3: 8748}, 8748, 8748, 256),
	"smp.image_management.ImageUploadWriteRequest": Pins({1: 34, 2: 395, 4: 4}, 80, 80, 3),
	"smp.image_management.ImageUploadWriteResponse": Pins({1: 2, 3: 75}, 61, 61, 8),
}


SMP_ONLY_RECORDS: Final = frozenset(["smp.os_management.MemoryPoolStatisticsReadResponse"])
