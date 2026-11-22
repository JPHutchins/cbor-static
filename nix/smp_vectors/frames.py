import importlib
import re
from typing import TYPE_CHECKING, Final

from smp import header as smpheader
from smp.message import _MessageBase

if TYPE_CHECKING:
	from pydantic import BaseModel

type Files = dict[str, bytes]


def require(holds: bool, why: str) -> None:
	if not holds:
		raise ValueError(why)


def snake(name: str) -> str:
	"""
	>>> [snake(name) for name in ("MCUMgrParametersReadRequest", "OSApplicationInfoReadRequest", "splitStatus")]
	['mcu_mgr_parameters_read_request', 'os_application_info_read_request', 'split_status']
	>>> snake("MCUbootModeReadRequest")
	'mc_uboot_mode_read_request'
	"""
	return re.sub(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", "_", name).lower()


def message_class(name: str) -> type[_MessageBase]:
	module, _, cls = name.rpartition(".")
	message: type[_MessageBase] = getattr(importlib.import_module(module), cls)
	return message


def payload_of(cls: type[_MessageBase], frame: bytes) -> bytes:
	"""The payload under `frame`'s header, once smp has parsed the header and it names `cls`."""
	header = smpheader.Header.loads(frame[: smpheader.Header.SIZE])
	require(
		(header.op, header.flags, header.group_id, header.command_id, header.length)
		== (cls._OP, cls._FLAGS, cls._GROUP_ID, cls._COMMAND_ID, len(frame) - smpheader.Header.SIZE),
		f"{header} for {cls.__name__} with {len(frame) - smpheader.Header.SIZE} payload bytes",
	)
	return frame[smpheader.Header.SIZE :]


def group(cls: type[BaseModel]) -> str:
	"""
	>>> from smp.image_management import ImageSlot
	>>> group(ImageSlot)
	'image'
	"""
	return cls.__module__.removeprefix("smp.").removesuffix("_management")


def qualified(cls: type[BaseModel]) -> str:
	return f"{cls.__module__}.{cls.__name__}"


TWIN: Final = ".cbor"


FRAME: Final = ".frame"


MESSAGE_FIELDS: Final = frozenset(_MessageBase.model_fields)
