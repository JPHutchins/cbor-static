"""cbor-static's serde layer, over minicbor-serde's types and attributes (R-301)."""

from typing import Final

from harness.backends.backend import (
	ROOT,
	SERDE,
	SERDE_BYTES,
	Backend,
	no_impls,
	path_dependency,
	roundtrip_fn,
	serde_map_field_attrs,
)
from harness.backends.serde import DERIVE, enum_def

CBOR_STATIC_SERDE: Final = Backend(
	name="cbor-static-serde",
	dependencies=lambda: "\n".join(
		(path_dependency("cbor-static", ROOT / "cbor-static", ("serde",)), SERDE, SERDE_BYTES)
	),
	prelude="",
	struct_attrs=DERIVE,
	field_attrs=serde_map_field_attrs,
	enum_def=enum_def,
	impls=no_impls,
	roundtrip=roundtrip_fn(
		"cbor_static::serde::from_slice::<{T}>(input)", "cbor_static::serde::to_slice(&m, out).ok()"
	),
)
