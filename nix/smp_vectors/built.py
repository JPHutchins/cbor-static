from typing import TYPE_CHECKING, Final

from frames import TWIN, payload_of, snake
from oracle import Generated, oracle_case
from rust import SMP_ONLY, data_file, generated, literal, long_bytes
from smp import image_management, os_management

if TYPE_CHECKING:
	from smp.message import _MessageBase


def hand_built() -> Generated:
	"""Each built message's payload as a twin, under the directory that says who built its shape,
	the generated cases that read them, and each as a case for the smp oracle.
	"""
	return Generated(
		{
			**{
				f"{directory}/{snake(type(message).__name__)}{TWIN}": payload_of(
					type(message), bytes(message)
				)
				for directory, message in BUILT
			},
			**{data_file(value): value for _, message in BUILT for value in long_bytes(message)},
			"hand_built.rs": generated(
				"hand_built",
				(
					(
						snake(type(message).__name__),
						f"{BUILT_MODULES[directory]}{type(message).__name__}",
						[
							f'        ("{directory}/{snake(type(message).__name__)}", {literal(message, BUILT_MODULES[directory])}),'
						],
					)
					for directory, message in BUILT
				),
			).encode(),
		},
		tuple(
			oracle_case(
				f"{directory}/{snake(type(message).__name__)}",
				type(message),
				BUILT_MODULES[directory],
				firmware=False,
			)
			for directory, message in BUILT
		),
	)


BUILT: Final[tuple[tuple[str, _MessageBase], ...]] = (
	("smp_only", os_management.ResetWriteRequest(force=1, sequence=0)),
	*(
		("hand_built", message)
		for message in (
			image_management.ImageStatesWriteRequest(hash=bytes(range(32)), confirm=True, sequence=0),
			image_management.ImageStatesWriteResponse(
				images=[
					image_management.ImageState(
						slot=0,
						version="1.2.3",
						hash=bytes(range(32)),
						bootable=True,
						confirmed=True,
						active=True,
					),
					image_management.ImageState(
						image=0,
						slot=1,
						version="1.3.0",
						hash=bytes(range(32, 64)),
						bootable=True,
						pending=True,
					),
				],
				splitStatus=0,
				sequence=0,
			),
			image_management.ImageSlotInfoReadRequest(sequence=0),
			image_management.ImageSlotInfoReadResponse(
				images=(
					image_management.ImageSlotInfo(
						image=0,
						slots=(
							image_management.ImageSlot(slot=0, size=0x60000, upload_image_id=0),
							image_management.ImageSlot(slot=1, size=0x60000, upload_image_id=1),
						),
						max_image_size=0x5F000,
					),
					image_management.ImageSlotInfo(
						image=1,
						slots=(
							image_management.ImageSlot(slot=0, size=0x20000),
							image_management.ImageSlot(slot=1, size=0x20000),
						),
					),
				),
				sequence=0,
			),
		)
	),
)


BUILT_MODULES: Final = {"smp_only": SMP_ONLY, "hand_built": ""}
