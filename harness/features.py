"""R-009: every vocabulary feature beyond the harness's is reached through its own ops tables. The
gate ELFs link none of them, a positive control links them all, and each feature's fixed and
per-type flash is its fixture's delta against a reference fixture that lacks only the feature.
"""

import argparse
import csv
import sys
import tomllib
from pathlib import Path
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.backend import ROOT, path_dependency
from harness.backends.cbor_static import CBOR_STATIC, ROUNDTRIP, roundtrip
from harness.elf import sizes
from harness.measure import CRATES, GATE_TARGET, HARNESS, PROFILES, build, elf_path, write_crate
from harness.model import Message
from harness.templates import render

if TYPE_CHECKING:
	from collections.abc import Sequence

DECODE: Final = "cbor_static::engine::decode"
ENCODE: Final = "cbor_static::engine::encode"
STRICT: Final = "cbor_static::engine::strict"
LEAF: Final = "cbor_static::engine::leaf"
STRINGS: Final = "cbor_static::engine::strings"
NIGHTLY: Final = "nightly"
HEAPLESS: Final = "heapless"
DECODE_ONLY: Final = "decode_only"
MANIFEST: Final = tomllib.loads((ROOT / "cbor-static" / "Cargo.toml").read_text())
FEATURES_DIR: Final = HARNESS / "features"
COUNTS: Final = (1, 8)


class Feature(NamedTuple):
	"""A feature's fixture template, the reference template it differs from only by the feature,
	the ops statics and handlers that implement it, the prelude function its types round-trip
	through, and the cargo features of cbor-static it needs. A cargo feature named after a toolchain
	builds on that toolchain, the feature's reference included.
	"""

	name: str
	template: str
	reference: str
	symbols: tuple[str, ...]
	decoding: str = ROUNDTRIP
	cargo_features: tuple[str, ...] = ()

	@property
	def nightly(self) -> bool:
		"""
		>>> BY_TEMPLATE["f16"].nightly, BY_TEMPLATE["half"].nightly
		(True, False)
		"""
		return NIGHTLY in self.cargo_features


FEATURES: Final = (
	Feature("integer keys", "int_keys", "struct", (f"{DECODE}::map::MAP_INT",)),
	Feature("enum keys (`keys = Label`)", "label_keys", "struct", (f"{DECODE}::map::MAP_INT",)),
	Feature("mixed keys", "mixed_keys", "struct", (f"{DECODE}::map::MAP_MIXED",)),
	Feature(
		"`deny_unknown_fields`, integer keys", "deny_int_keys", "struct", (f"{DECODE}::map::MAP_INT_DENY",)
	),
	Feature("`deny_unknown_fields`, text keys", "deny_text_keys", "struct", (f"{DECODE}::map::MAP_DENY",)),
	Feature(
		"`#[cbor(array)]`", "array_struct", "struct", (f"{DECODE}::array::ARRAY", f"{ENCODE}::array::ARRAY")
	),
	Feature("tuple", "tuple", "struct", (f"{DECODE}::array::ARRAY", f"{ENCODE}::array::ARRAY")),
	Feature("tuple struct", "tuple_struct", "struct", (f"{DECODE}::array::ARRAY", f"{ENCODE}::array::ARRAY")),
	Feature(
		"unit and newtype structs",
		"shapes",
		"struct",
		tuple(f"{LEAF}::{kind}_scalar::<()>" for kind in ("enc", "dec")),
	),
	Feature(
		"`[T; N]`",
		"fixed_array",
		"struct",
		(f"{DECODE}::elements::ELEMENTS", f"{ENCODE}::elements::ELEMENTS"),
	),
	Feature(
		"`Option<nested>`",
		"option_nested",
		"nested",
		(f"{DECODE}::option::OPTION", f"{ENCODE}::option::OPTION"),
	),
	Feature("integer index", "index", "enum", (f"{DECODE}::index::INDEX", f"{ENCODE}::index::INDEX")),
	Feature(
		"CBOR tag",
		"cbor_tag",
		"enum_payloads",
		(f"{DECODE}::cbor_tag::CBOR_TAG", f"{ENCODE}::cbor_tag::CBOR_TAG"),
	),
	Feature(
		"internally tagged",
		"internal",
		"enum_unit_struct",
		(f"{DECODE}::tagged::INTERNAL", f"{ENCODE}::tagged::INTERNAL"),
	),
	Feature(
		"adjacently tagged",
		"adjacent",
		"enum",
		(f"{DECODE}::tagged::ADJACENT", f"{ENCODE}::tagged::ADJACENT"),
	),
	Feature(
		"untagged", "untagged", "enum", (f"{DECODE}::untagged::UNTAGGED", f"{ENCODE}::untagged::UNTAGGED")
	),
	Feature(
		"`Chunks`",
		"chunks",
		"strings",
		(
			"cbor_static::engine::skip::rest_len",
			f"{ENCODE}::chunks::TEXT",
			f"{ENCODE}::chunks::BYTES",
		),
	),
	Feature(
		"`Strict` profile",
		"strict",
		"struct",
		tuple(
			f"{STRICT}::{name}"
			for name in ("STRICT", "head", "order", "bignum", "unknown", "sorted", "raw", "keys")
		),
		"roundtrip_strict",
	),
	Feature(
		"8-, 16- and 64-bit integers, `usize`, `isize`",
		"ints",
		"struct",
		tuple(f"{LEAF}::{kind}_scalar::<{ty}>" for kind in ("enc", "dec") for ty in ("u8", "i64")),
	),
	Feature(
		"`F16`",
		"half",
		"struct",
		tuple(f"{LEAF}::{kind}_scalar::<cbor_static::half::F16>" for kind in ("enc", "dec")),
	),
	Feature(
		"`f16` (nightly)",
		"f16",
		"half",
		tuple(f"{LEAF}::{kind}_scalar::<f16>" for kind in ("enc", "dec")),
		cargo_features=(NIGHTLY,),
	),
	Feature(
		"`char`",
		"chars",
		"struct",
		(f"{DECODE}::owned::TEXT", f"{STRINGS}::ENC_CHAR", f"{STRINGS}::DEC_CHAR"),
	),
	Feature(
		"`[u8; N]` under `#[cbor(bytes)]`",
		"byte_array",
		"struct",
		(f"{DECODE}::owned::BYTES", f"{STRINGS}::within::<4>", f"{STRINGS}::exactly::<4>"),
	),
	Feature(
		"heapless `String`",
		"heapless_string",
		"struct",
		(f"{DECODE}::owned::TEXT", f"{STRINGS}::bounded::enc_string::<8, u8>"),
		cargo_features=(HEAPLESS,),
	),
	Feature(
		"heapless `Vec<u8>` under `#[cbor(bytes)]`",
		"heapless_bytes",
		"struct",
		(f"{DECODE}::owned::BYTES", f"{STRINGS}::bounded::append_vec::<8, u8>"),
		cargo_features=(HEAPLESS,),
	),
	Feature(
		"`Tagged` over any value",
		"tagged",
		"struct",
		(f"{ENCODE}::tag::TAG", f"{LEAF}::dec_tagged::<1, u32>"),
	),
	Feature(
		"`SelfDescribed`",
		"self_described",
		"struct",
		(f"{ENCODE}::tag::TAG", f"{DECODE}::described::SELF_DESCRIBED"),
	),
	Feature("`RawCbor`", "raw", "struct", (f"{DECODE}::raw::RAW", "cbor_static::engine::raw::ENC_RAW")),
	Feature(
		"`ArrayRef`",
		"array_ref",
		"struct",
		(f"{DECODE}::raw::RAW", "cbor_static::engine::raw::store_array::<u32>"),
	),
	Feature(
		"`MapRef`",
		"map_ref",
		"struct",
		(f"{DECODE}::raw::RAW", "cbor_static::engine::raw::store_map::<u8, u32>"),
	),
	Feature(
		"heapless `Vec`",
		"heapless_vec",
		"struct",
		(f"{DECODE}::seq::SEQ", f"{ENCODE}::seq::SEQ"),
		cargo_features=(HEAPLESS,),
	),
	Feature(
		"heapless `LinearMap`",
		"heapless_map",
		"struct",
		(f"{DECODE}::linear_map::LINEAR_MAP", f"{ENCODE}::linear_map::LINEAR_MAP"),
		cargo_features=(HEAPLESS,),
	),
	Feature(
		"heapless `LinearMap`, decode-only",
		"heapless_map_decode",
		"struct_decode",
		(f"{DECODE}::linear_map::LINEAR_MAP",),
		DECODE_ONLY,
		(HEAPLESS,),
	),
)
REFERENCE_DECODINGS: Final = {"struct_decode": DECODE_ONLY}
BY_TEMPLATE: Final = {feature.template: feature for feature in FEATURES}


class Cost(NamedTuple):
	feature: str
	profile: str
	fixed: int
	per_type: float


def main(argv: Sequence[str]) -> int:
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--feature", choices=list(BY_TEMPLATE), required=True)
	feature = BY_TEMPLATE[parser.parse_args(argv).feature]
	costs = [cost(feature, profile) for profile in PROFILES]
	FEATURES_DIR.mkdir(parents=True, exist_ok=True)
	with (FEATURES_DIR / f"{feature.template}.csv").open("w", newline="") as handle:
		writer = csv.writer(handle)
		writer.writerow(Cost._fields)
		writer.writerows(costs)
	for row in costs:
		print(row)
	return 0


def cost(feature: Feature, profile: str) -> Cost:
	"""The feature's fixed flash at N=1 and its per-type flash from N=1 to N=8, both as deltas
	against the reference.
	"""
	fixed, at_eight = (
		flash(feature.template, count, feature, profile) - flash(feature.reference, count, feature, profile)
		for count in COUNTS
	)
	return Cost(feature.name, profile, fixed, (at_eight - fixed) / (COUNTS[1] - COUNTS[0]))


def flash(template: str, count: int, feature: Feature, profile: str) -> int:
	"""Flash of a firmware round-tripping `count` types made from `template`, built as `feature`'s."""
	return sizes(
		fixture(
			fixture_name(template, count),
			(template,) * count,
			feature.template,
			profile,
			feature.cargo_features,
		)
	).flash


def decoding(template: str) -> str:
	"""
	>>> decoding("strict"), decoding("struct"), decoding("struct_decode")
	('roundtrip_strict', 'roundtrip', 'decode_only')
	"""
	return (
		BY_TEMPLATE[template].decoding
		if template in BY_TEMPLATE
		else REFERENCE_DECODINGS.get(template, ROUNDTRIP)
	)


def fixture_name(template: str, count: int) -> str:
	return f"r009-{template}-n{count}"


def built() -> tuple[Path, ...]:
	"""The ELFs `cost` built for every feature and its reference, where a run built them."""
	return tuple(
		filter(
			Path.is_file,
			(
				elf_path(FEATURES_DIR / feature.template, GATE_TARGET, profile, fixture_name(template, count))
				for feature in FEATURES
				for template in (feature.template, feature.reference)
				for count in COUNTS
				for profile in PROFILES
			),
		)
	)


def fixture(
	name: str, templates: Sequence[str], cell: str, profile: str, cargo_features: tuple[str, ...] = ()
) -> Path:
	"""Builds a firmware whose type `T{i}` comes from `templates[i]`, with `cargo_features` of
	cbor-static; returns its ELF.
	"""
	target_dir = FEATURES_DIR / cell
	build(
		write_crate(
			CRATES / "features" / name,
			{
				"Cargo.toml": render("Cargo.toml", name=name, dependencies=dependencies(cargo_features)),
				**pinned(cargo_features),
				"src/main.rs": render(
					"main.rs",
					attributes="#![feature(f16)]" if NIGHTLY in cargo_features else "",
					prelude=CBOR_STATIC.prelude,
					items="\n".join(
						render(f"features/{template}.rs", i=str(index))
						+ "\n"
						+ roundtrip(Message(f"T{index}", ()), decoding(template))
						for index, template in enumerate(templates)
					),
					calls="\n".join(
						f"    black_box(roundtrip_t{index}(input, out));" for index in range(len(templates))
					),
				),
			},
		),
		GATE_TARGET,
		profile,
		(),
		target_dir,
	)
	return elf_path(target_dir, GATE_TARGET, profile, name)


def dependencies(cargo_features: tuple[str, ...]) -> str:
	"""cbor-static with `cargo_features`, and each crate they enable at cbor-static's own version of
	it, so the firmware can name its types.

	>>> print(dependencies((HEAPLESS,)).splitlines()[1])
	heapless = { version = "0.9.3", default-features = false }
	"""
	return "\n".join(
		(
			path_dependency("cbor-static", ROOT / "cbor-static", cargo_features),
			*(
				f'{crate} = {{ version = "{MANIFEST["dependencies"][crate]["version"]}", default-features = false }}'
				for crate in (
					enabled.removeprefix("dep:")
					for feature in cargo_features
					for enabled in MANIFEST["features"][feature]
					if enabled.startswith("dep:")
				)
			),
		)
	)


def pinned(cargo_features: tuple[str, ...]) -> dict[str, str]:
	"""The `rust-toolchain.toml` that selects the nightly toolchain, for a crate that needs it: the
	toolchain proxy takes the one nearest the crate.

	>>> pinned(()), "channel" in pinned((NIGHTLY,))["rust-toolchain.toml"]
	({}, True)
	"""
	return (
		{"rust-toolchain.toml": (ROOT / NIGHTLY / "rust-toolchain.toml").read_text()}
		if NIGHTLY in cargo_features
		else {}
	)


def load() -> list[Cost]:
	return [
		Cost(row["feature"], row["profile"], int(row["fixed"]), float(row["per_type"]))
		for feature in FEATURES
		for row in csv.DictReader((FEATURES_DIR / f"{feature.template}.csv").read_text().splitlines())
	]


if __name__ == "__main__":
	sys.exit(main(sys.argv[1:]))
