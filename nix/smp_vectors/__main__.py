"""SMP vectors as binary twins of their CBOR payloads, and the generated cases that read them.

JPHutchins/smp's own message classes, built from the same pinned revision as the records, validate
every record: the class named by the record parses its 8-byte header, rebuilds the frozen bytes
from the record's kwargs, and dumps back to those kwargs. Hex strings in the kwargs decode as
smp's tests/binary_regressions/test_binary_lock.py decodes them. Any record, header or array that
differs from the pins in `records.py` and `zephyr.py` fails the build.
"""

from pathlib import Path
from typing import TYPE_CHECKING, Annotated, Final

from built import hand_built
from cyclopts import App, Parameter
from oracle import oracle_files
from records import module_records
from zephyr import zephyr

if TYPE_CHECKING:
	from oracle import Generated

MODULES: Final = ("os_management", "image_management")


app = App(help=__doc__)


@app.default
def main(
	records: Annotated[Path, Parameter(help="JPHutchins/smp's tests/binary_regressions/records.")],
	echo: Annotated[Path, Parameter(help="Zephyr's tests/subsys/mgmt/mcumgr/os_mgmt_echo/src/main.c.")],
	info: Annotated[Path, Parameter(help="Zephyr's tests/subsys/mgmt/mcumgr/os_mgmt_info/src/main.c.")],
	smp_version: Annotated[Path, Parameter(help="Zephyr's tests/subsys/mgmt/mcumgr/smp_version/src/main.c.")],
	*,
	smp_rev: Annotated[
		str, Parameter(help="The JPHutchins/smp revision the records and smp are built from.")
	],
	out: Annotated[Path, Parameter(help="The directory the twins and generated cases are written to.")],
) -> None:
	parts: tuple[Generated, ...] = (
		*(module_records(records, module) for module in MODULES),
		zephyr({"os_mgmt_echo": echo, "os_mgmt_info": info, "smp_version": smp_version}),
		hand_built(),
	)
	for name, data in {
		**{name: data for part in parts for name, data in part.files.items()},
		**oracle_files((case for part in parts for case in part.oracle), smp_rev),
	}.items():
		(out / name).parent.mkdir(parents=True, exist_ok=True)
		(out / name).write_bytes(data)


if __name__ == "__main__":
	app()
