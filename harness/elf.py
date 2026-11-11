"""Section headers and symbol bytes of little-endian ELF files, read directly (no tool output parsing)."""

import struct
import subprocess
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.llvm import llvm_tool

if TYPE_CHECKING:
	from pathlib import Path

SHF_ALLOC: Final = 0x2
SHT_NOBITS: Final = 8
RESEARCH_SECTIONS: Final = (".text", ".rodata", ".data")


class Sizes(NamedTuple):
	flash: int
	research: int


class Section(NamedTuple):
	name: str
	kind: int
	flags: int
	address: int
	offset: int
	size: int

	@property
	def occupies_flash(self) -> bool:
		"""Allocated and stored in the image: what a flash budget pays for.

		>>> Section(".rodata", 1, SHF_ALLOC, 0, 0, 8).occupies_flash
		True
		>>> Section(".bss", SHT_NOBITS, SHF_ALLOC | 1, 0, 0, 8).occupies_flash
		False
		>>> Section(".comment", 1, 0x30, 0, 0, 8).occupies_flash
		False
		"""
		return bool(self.flags & SHF_ALLOC) and self.kind != SHT_NOBITS


class Symbol(NamedTuple):
	name: str
	address: int
	size: int


def sizes(elf: Path) -> Sizes:
	found = sections(elf.read_bytes())
	return Sizes(
		flash=sum(s.size for s in found if s.occupies_flash),
		research=sum(s.size for s in found if s.name in RESEARCH_SECTIONS),
	)


def symbol_bytes(elf: Path, symbol: Symbol) -> bytes:
	data = elf.read_bytes()
	section = next(s for s in sections(data) if s.address <= symbol.address < s.address + s.size)
	start = section.offset + symbol.address - section.address
	return data[start : start + symbol.size]


def symbols(toolchain_dir: Path, elf: Path) -> tuple[Symbol, ...]:
	"""Defined symbols with sizes, demangled (v0 mangling keeps generic arguments)."""
	listing = subprocess.run(
		[str(llvm_tool(toolchain_dir, "llvm-nm")), "--defined-only", "--print-size", "--demangle", str(elf)],
		capture_output=True,
		text=True,
		check=True,
	).stdout
	return tuple(
		Symbol(fields[3], int(fields[0], 16), int(fields[1], 16))
		for fields in (line.split(maxsplit=3) for line in listing.splitlines())
		if len(fields) == 4
	)


def sections(data: bytes) -> tuple[Section, ...]:
	wide = data[4] == 2
	shoff, shentsize, shnum, shstrndx = (
		struct.unpack_from("<Q", data, 0x28)[0] if wide else struct.unpack_from("<I", data, 0x20)[0],
		*struct.unpack_from("<HHH", data, 0x3A if wide else 0x2E),
	)
	header = "<IIQQQQ" if wide else "<IIIIII"
	raw = tuple(struct.unpack_from(header, data, shoff + index * shentsize) for index in range(shnum))
	names_offset = raw[shstrndx][4]
	return tuple(
		Section(
			data[names_offset + name : data.index(b"\0", names_offset + name)].decode(),
			kind,
			flags,
			address,
			offset,
			size,
		)
		for name, kind, flags, address, offset, size in raw
	)
