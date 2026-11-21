//! RFC 9529 §4's invalid items, one case each. RFC 9528 §9.8: "implementations MUST abort the
//! EDHOC session if the message does not comply with the CDDL for that message. Implementations
//! are not required to support non-deterministic encodings and MAY abort the EDHOC session if the
//! received EDHOC message is not encoded using deterministic CBOR. Implementations MUST abort the
//! EDHOC session if validation of a received public key fails or if any cryptographic field has
//! the wrong length." The typed layer rejects what its types can express; each item it accepts is
//! pinned with the reason a receiver still aborts.

use cbor_static::ErrorKind;

use crate::items::{Item, reencoded};

/// Why a receiver aborts on an item the typed layer accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Accepted {
    /// The CDDL's minimum count, which the derive cannot express yet (#64).
    MinimumCount,
    /// A prose encoding rule of RFC 9528 §3.3.2 or §3.5.3.2 that its CDDL does not state.
    ProseEncodingRule,
    /// A public key or cryptographic length check.
    Cryptography,
    /// A non-deterministic encoding: the default profile reads it, the Strict profile rejects the
    /// item alone, and no Strict decode of a sequence exists yet (#64).
    NotDeterministic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Rejected(ErrorKind),
    Accepted(Accepted),
}

const VERDICTS: [(&str, Verdict); 15] = [
    ("4.1.1", Verdict::Rejected(ErrorKind::TypeMismatch)),
    ("4.1.2", Verdict::Accepted(Accepted::ProseEncodingRule)),
    ("4.1.3", Verdict::Accepted(Accepted::MinimumCount)),
    ("4.1.4", Verdict::Rejected(ErrorKind::TypeMismatch)),
    ("4.1.5", Verdict::Rejected(ErrorKind::TrailingInput)),
    ("4.1.6", Verdict::Accepted(Accepted::ProseEncodingRule)),
    ("4.1.7", Verdict::Accepted(Accepted::ProseEncodingRule)),
    ("4.2.1", Verdict::Accepted(Accepted::Cryptography)),
    ("4.2.2", Verdict::Accepted(Accepted::Cryptography)),
    ("4.2.3", Verdict::Accepted(Accepted::Cryptography)),
    ("4.2.4", Verdict::Accepted(Accepted::Cryptography)),
    ("4.2.5", Verdict::Accepted(Accepted::Cryptography)),
    ("4.2.6", Verdict::Accepted(Accepted::Cryptography)),
    ("4.3.1", Verdict::Accepted(Accepted::NotDeterministic)),
    ("4.3.2", Verdict::Accepted(Accepted::NotDeterministic)),
];

/// The typed layer's verdict on the whole item, as the type its caption names.
fn decoded(item: Item, input: &[u8]) -> Result<(), ErrorKind> {
    reencoded(item, input).map(|_| ()).map_err(|e| e.kind)
}

/// The Strict profile's verdict on each item of the sequence alone.
fn strict_verdicts(input: &[u8]) -> Vec<Result<(), ErrorKind>> {
    use cbor_static::RawCbor;

    core::iter::successors(
        cbor_static::from_slice_partial::<RawCbor>(input).ok(),
        |(_, rest)| cbor_static::from_slice_partial::<RawCbor>(rest).ok(),
    )
    .map(|(raw, _)| {
        cbor_static::decode_strict_into::<RawCbor>(
            raw.as_bytes(),
            &mut core::mem::MaybeUninit::uninit(),
        )
        .map(|_| ())
        .map_err(|e| e.kind)
    })
    .collect()
}

fn verdict(section: &str, item: Item, input: &[u8]) {
    match VERDICTS
        .iter()
        .find(|(listed, _)| *listed == section)
        .map(|(_, expected)| *expected)
    {
        Some(Verdict::Rejected(kind)) => assert_eq!(decoded(item, input), Err(kind)),
        Some(Verdict::Accepted(why)) => {
            assert_eq!(decoded(item, input), Ok(()));
            if why == Accepted::NotDeterministic {
                assert!(matches!(reencoded(item, input), Ok(written) if written != input));
                assert_eq!(
                    strict_verdicts(input)
                        .into_iter()
                        .filter(|verdict| *verdict == Err(ErrorKind::NotDeterministic))
                        .count(),
                    1
                );
            }
        }
        None => panic!("no verdict for {section}"),
    }
}

macro_rules! rfc9529_invalid_cases {
    ($(($test:ident, $section:literal, $item:ident)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                verdict($section, Item::$item, rfc9529!($test));
            }
        )*

        const SECTIONS: &[&str] = &[$($section),*];
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc9529_invalid_cases.rs"));

#[test]
fn each_verdict_is_one_invalid_item() {
    assert_eq!(VERDICTS.map(|(section, _)| section).as_slice(), SECTIONS);
}

#[test]
fn three_invalid_items_are_rejected_and_twelve_pinned() {
    let count = |wanted: fn(&Verdict) -> bool| VERDICTS.iter().filter(|(_, v)| wanted(v)).count();
    assert_eq!(
        [
            count(|v| matches!(v, Verdict::Rejected(_))),
            count(|v| *v == Verdict::Accepted(Accepted::MinimumCount)),
            count(|v| *v == Verdict::Accepted(Accepted::ProseEncodingRule)),
            count(|v| *v == Verdict::Accepted(Accepted::Cryptography)),
            count(|v| *v == Verdict::Accepted(Accepted::NotDeterministic)),
        ],
        [3, 1, 3, 6, 2]
    );
}
