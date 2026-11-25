#[allow(dead_code)]
#[path = "$common"]
mod oracles;

mod common;

use common::{ciborium_bytes, serde_layer, stat};
use differential::{flat, rich};
use oracles::typed::round_trip;

/// Every oracle reads what cbor-static writes as `Tokens` does, and each one's re-encoding reads
/// back as the same bytes; ciborium serializes the value to the serde layer's bytes.
macro_rules! differential {
    ($$ty:ty, $$value:expr) => {{
        let value = $$value;
        round_trip(&stat(&value), |bytes| cbor_static::from_slice::<$$ty>(bytes).map(|decoded| stat(&decoded)));
        assert_eq!(ciborium_bytes(&value), serde_layer(&value));
    }};
}

/// ciborium deserializes what cbor-static writes into the type, which writes the same bytes; only
/// for a type that borrows nothing, as ciborium cannot lend from its input.
macro_rules! owned {
    ($$ty:ty, $$value:expr) => {{
        let native = stat(&$$value);
        let decoded: $$ty = ciborium::from_reader(native.as_slice()).expect("ciborium deserializes cbor-static's bytes");
        assert_eq!(stat(&decoded), native);
    }};
}

$tests
