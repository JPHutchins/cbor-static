import re
from functools import reduce
from typing import TYPE_CHECKING, Final, NamedTuple

from frames import FRAME, TWIN, payload_of, require
from oracle import Generated, oracle_case
from smp.os_management import (
	EchoWriteRequest,
	EchoWriteResponse,
	OSApplicationInfoReadRequest,
	OSApplicationInfoReadResponse,
	OSManagementErrorV1,
	OSManagementErrorV2,
)

if TYPE_CHECKING:
	from pathlib import Path

	from smp.message import _MessageBase


def zephyr(sources: dict[str, Path]) -> Generated:
	"""Each pinned array's payload as a twin, its whole frame beside it, and each as a case for the
	smp oracle.
	"""
	arrays = {
		(test, name, branch): data
		for test, path in sources.items()
		for (name, branch), data in braced_arrays(path.read_text()).items()
	}
	pinned = {(array.test, array.name, array.branch): array for array in ZEPHYR_ARRAYS}
	require(arrays.keys() == pinned.keys(), f"zephyr: pinned {sorted(pinned)}, found {sorted(arrays)}")
	require(
		all(len(data) == pinned[key].length for key, data in arrays.items()),
		f"zephyr: pinned {[(key, array.length) for key, array in pinned.items()]}, found "
		f"{[(key, len(data)) for key, data in arrays.items()]}",
	)
	return Generated(
		{
			**{
				f"{array.twin}{TWIN}": payload_of(array.command, arrays[key]) for key, array in pinned.items()
			},
			**{f"{array.twin}{FRAME}": arrays[key] for key, array in pinned.items()},
		},
		tuple(oracle_case(array.twin, array.mirrors, "", firmware=True) for array in ZEPHYR_ARRAYS),
	)


def braced_arrays(source: str) -> dict[tuple[str, str], bytes]:
	r"""Each `static const uint8_t` array, keyed by its name and the preprocessor branch it is in.

	>>> braced_arrays("#ifdef A\nstatic const uint8_t x[] = { 0x01 };\n#else\nstatic const uint8_t x[] = { 0x02 };\n#endif")
	{('x', 'A'): b'\x01', ('x', '!A'): b'\x02'}
	>>> sorted(braced_arrays(
	...     "#if A\n#elif B\nstatic const uint8_t y[] = { 0x03 };\n#else\nstatic const uint8_t z[] = { 0x04 };\n#endif\n"
	... ))
	[('y', '!A && B'), ('z', '!A && !B')]
	"""
	arrays = reduce(scanned, SOURCE_TOKEN.finditer(source), Scan((), ())).arrays
	keyed = {(name, branch): body for name, branch, body in arrays}
	require(len(keyed) == len(arrays), f"an array is defined twice in one branch: {[a[:2] for a in arrays]}")
	return keyed


def scanned(scan: Scan, token: re.Match[str]) -> Scan:
	opened, innermost = scan.levels, scan.levels[-1:]
	match token["directive"], (token["condition"] or "").strip():
		case "ifdef" | "if", condition:
			return scan._replace(levels=(*opened, Level((condition,), otherwise=False)))
		case "ifndef", condition:
			return scan._replace(levels=(*opened, Level((negated(condition),), otherwise=False)))
		case "elif", condition:
			return scan._replace(
				levels=(*opened[:-1], *(Level((*level.conditions, condition), False) for level in innermost))
			)
		case "else", _:
			return scan._replace(
				levels=(*opened[:-1], *(level._replace(otherwise=True) for level in innermost))
			)
		case "endif", _:
			return scan._replace(levels=opened[:-1])
		case _:
			return scan._replace(
				arrays=(
					*scan.arrays,
					(
						token["name"],
						" && ".join(level.holds() for level in opened),
						bytes(c_byte(byte.strip()) for byte in token["body"].split(",") if byte.strip()),
					),
				)
			)


class Scan(NamedTuple):
	levels: tuple[Level, ...]
	arrays: tuple[tuple[str, str, bytes], ...]


class Level(NamedTuple):
	"""One open `#if`: the conditions of its branches so far, and whether it is in its `#else`."""

	conditions: tuple[str, ...]
	otherwise: bool

	def holds(self) -> str:
		taken = self.conditions if self.otherwise else self.conditions[:-1]
		return " && ".join([*map(negated, taken), *(() if self.otherwise else self.conditions[-1:])])


def negated(condition: str) -> str:
	"""
	>>> negated("CONFIG_A"), negated("!CONFIG_A"), negated("defined(A) || defined(B)")
	('!CONFIG_A', 'CONFIG_A', '!(defined(A) || defined(B))')
	"""
	if re.fullmatch(r"!\w+", condition):
		return condition.removeprefix("!")
	return f"!{condition}" if re.fullmatch(r"[\w()]+", condition) else f"!({condition})"


def c_byte(token: str) -> int:
	if not re.fullmatch(r"0x[0-9a-f]{2}", token):
		raise ValueError(f"{token!r} is not a byte")
	return int(token, 16)


class CArray(NamedTuple):
	"""A firmware frame in a Zephyr test: its array, the preprocessor branch that defines it, the
	command its header names, and the smp class its payload is read as.
	"""

	test: str
	name: str
	branch: str
	length: int
	command: type[_MessageBase]
	mirrors: type[_MessageBase]

	@property
	def twin(self) -> str:
		return f"zephyr/{self.test}/{self.name}{BRANCH_SUFFIXES.get(self.branch, '')}"


ORIGINAL_PROTOCOL: Final = "CONFIG_MCUMGR_SMP_SUPPORT_ORIGINAL_PROTOCOL"


INFO_TEST_BUILD: Final = "!defined(CONFIG_BUILD_DATE_TIME_TEST) && !defined(CONFIG_LIMITED_TEST)"


BRANCH_SUFFIXES: Final = {ORIGINAL_PROTOCOL: "_original_protocol"}


ZEPHYR_ARRAYS: Final = (
	CArray("os_mgmt_echo", "command", "", 54, EchoWriteRequest, EchoWriteRequest),
	CArray("os_mgmt_echo", "expected_response", "", 54, EchoWriteResponse, EchoWriteResponse),
	CArray("os_mgmt_echo", "command_no_data", "", 10, EchoWriteRequest, EchoWriteRequest),
	CArray("os_mgmt_echo", "expected_response_no_data", "", 14, EchoWriteResponse, OSManagementErrorV1),
	CArray(
		"os_mgmt_info",
		"command",
		INFO_TEST_BUILD,
		19,
		OSApplicationInfoReadRequest,
		OSApplicationInfoReadRequest,
	),
	CArray(
		"os_mgmt_info",
		"expected_response",
		INFO_TEST_BUILD,
		24,
		OSApplicationInfoReadResponse,
		OSApplicationInfoReadResponse,
	),
	CArray(
		"smp_version",
		"response_old",
		ORIGINAL_PROTOCOL,
		14,
		OSApplicationInfoReadResponse,
		OSManagementErrorV1,
	),
	CArray(
		"smp_version",
		"response_old",
		f"!{ORIGINAL_PROTOCOL}",
		14,
		OSApplicationInfoReadResponse,
		OSManagementErrorV1,
	),
	CArray("smp_version", "response_current", "", 27, OSApplicationInfoReadResponse, OSManagementErrorV2),
	CArray("smp_version", "response_new", "", 14, OSApplicationInfoReadResponse, OSManagementErrorV1),
)


SOURCE_TOKEN: Final = re.compile(
	r"^#(?P<directive>ifdef|ifndef|if|elif|else|endif)\b[ \t]*(?P<condition>.*)$"
	r"|static const uint8_t (?P<name>\w+)\[\] = \{(?P<body>[^}]*)\};",
	re.MULTILINE,
)
