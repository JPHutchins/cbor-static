"""Codec backends: how each crate spells message structs, enums and a decode-then-encode round trip."""

from typing import TYPE_CHECKING, Final

from harness.backends.backend import rust_type as rust_type
from harness.backends.cbor_static import VARIANTS
from harness.backends.minicbor import MINICBOR_HAND_TEXTKEYS
from harness.backends.serde import MINICBOR_SERDE, POSTCARD

if TYPE_CHECKING:
	from collections.abc import Mapping

	from harness.backends.backend import Backend

BACKENDS: Final[Mapping[str, Backend]] = {
	backend.name: backend for backend in (*VARIANTS, MINICBOR_HAND_TEXTKEYS, MINICBOR_SERDE, POSTCARD)
}
