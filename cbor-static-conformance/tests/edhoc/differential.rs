//! What the EDHOC types write, through ciborium, minicbor and cbor2: every RFC 9529 trace item as
//! its type re-encodes it, a group item by item, one case per item.

use crate::common::differential::typed::round_trip;
use crate::items::{Item, reencoded};

fn item_round_trips(item: Item, input: &[u8]) {
    round_trip(&reencoded(item, input).expect("re-encodes"), |bytes| {
        reencoded(item, bytes)
    });
}

macro_rules! rfc9529_cases {
    ($(($test:ident, $section:literal, $item:ident)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                item_round_trips(Item::$item, rfc9529!($test));
            }
        )*
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc9529_cases.rs"));
