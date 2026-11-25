//! What the CWT types write, through ciborium, minicbor and cbor2: each example as its section's
//! type re-encodes it, and the claims set a signed or authenticated example carries, one case per
//! example and item.

use crate::common::differential::typed::round_trip;
use crate::common::reencode;
use crate::examples::{Section, claims_payload, message, reencoded};

fn example_round_trips(section: Section, input: &[u8]) {
    round_trip(&reencoded(section, input).expect("re-encodes"), |bytes| {
        reencoded(section, bytes)
    });
}

fn claims_round_trip(section: Section, input: &[u8]) {
    use cbor_static_conformance::cwt::Claims;

    if let Some(payload) = message(section, input).as_ref().and_then(claims_payload) {
        round_trip(&reencode::<Claims>(payload).expect("re-encodes"), |bytes| {
            reencode::<Claims>(bytes)
        });
    }
}

macro_rules! rfc8392_cases {
    ($(($test:ident, $section:literal, $variant:ident)),* $(,)?) => {
        $(
            mod $test {
                use super::Section;

                #[test]
                fn example() {
                    super::example_round_trips(Section::$variant, rfc8392!($test));
                }

                #[test]
                fn claims() {
                    super::claims_round_trip(Section::$variant, rfc8392!($test));
                }
            }
        )*
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc8392_cases.rs"));
