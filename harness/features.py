"""R-009: every vocabulary feature beyond the harness's is reached through its own ops tables. The
gate ELFs link none of them, a positive control links them all, and each feature's fixed and
per-type flash is its fixture's delta against a reference fixture that lacks only the feature.
"""

import argparse
import csv
import sys
from typing import TYPE_CHECKING, Final, NamedTuple

from harness.backends.cbor_static import CBOR_STATIC
from harness.elf import sizes
from harness.measure import CRATES, GATE_TARGET, HARNESS, PROFILES, build, elf_path, write_crate
from harness.model import Message
from harness.templates import render

if TYPE_CHECKING:
	from collections.abc import Sequence
	from pathlib import Path

DECODE: Final = "cbor_static::engine::decode"
ENCODE: Final = "cbor_static::engine::encode"
FEATURES_DIR: Final = HARNESS / "features"
COUNTS: Final = (1, 8)


class Feature(NamedTuple):
	"""A feature's fixture template, the reference template it differs from only by the feature,
	and the ops statics that implement it.
	"""

	name: str
	template: str
	reference: str
	symbols: tuple[str, ...]


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
)
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
		flash(feature.template, count, feature.template, profile)
		- flash(feature.reference, count, feature.template, profile)
		for count in COUNTS
	)
	return Cost(feature.name, profile, fixed, (at_eight - fixed) / (COUNTS[1] - COUNTS[0]))


def flash(template: str, count: int, cell: str, profile: str) -> int:
	"""Flash of a firmware round-tripping `count` types made from `template`."""
	return sizes(fixture(f"r009-{template}-n{count}", (template,) * count, cell, profile)).flash


def fixture(name: str, templates: Sequence[str], cell: str, profile: str) -> Path:
	"""Builds a firmware whose type `T{i}` comes from `templates[i]`; returns its ELF."""
	target_dir = FEATURES_DIR / cell
	build(
		write_crate(
			CRATES / "features" / name,
			{
				"Cargo.toml": render("Cargo.toml", name=name, dependencies=CBOR_STATIC.dependencies),
				"src/main.rs": render(
					"main.rs",
					prelude=CBOR_STATIC.prelude,
					items="\n".join(
						render(f"features/{template}.rs", i=str(index))
						+ "\n"
						+ CBOR_STATIC.roundtrip(Message(f"T{index}", ()))
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


def load() -> list[Cost]:
	return [
		Cost(row["feature"], row["profile"], int(row["fixed"]), float(row["per_type"]))
		for feature in FEATURES
		for row in csv.DictReader((FEATURES_DIR / f"{feature.template}.csv").read_text().splitlines())
	]


if __name__ == "__main__":
	sys.exit(main(sys.argv[1:]))
