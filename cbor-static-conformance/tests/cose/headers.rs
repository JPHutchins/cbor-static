//! Header maps at every layer: as written, through the order-preserving map, and typed, compared
//! as data with it.

use cbor_static_conformance::cose::{HeaderMap, Headers, Label};

use crate::common::vectors::{Kind, Vector, vector};
use crate::common::{encode, reencode};
use crate::decoded::{intact, layers};

/// Every header map in an example, as written: each layer's protected content and its
/// unprotected map. An example whose tag the generator broke has none to check.
pub fn maps(vector: &Vector<Kind>) -> Vec<(String, Vec<u8>)> {
    intact(vector).map_or_else(Vec::new, |decoded| {
        layers(&decoded)
            .into_iter()
            .flat_map(|layer| {
                [
                    (
                        format!("{at} protected", at = layer.at),
                        layer.protected.0.to_vec(),
                    ),
                    (
                        format!("{at} unprotected", at = layer.at),
                        encode(&layer.unprotected).expect("encodes"),
                    ),
                ]
            })
            .filter(|(_, bytes)| !bytes.is_empty())
            .collect()
    })
}

/// The typed struct's field order: RFC 9052 §3.1's labels as declared, then everything else.
fn declared(label: &Label) -> Option<i64> {
    match label {
        Label::Int(known @ 1..=6) => Some(*known),
        Label::Int(_) | Label::Text(_) => None,
    }
}

/// The maps RFC 9052 §3.1's types reject: cose-wg's X.509 examples write `kid` as text, where
/// the RFC has `bstr`.
const TEXT_KID: [(&str, &str); 2] = [
    ("x509-examples/signed-01", "signers.0 unprotected"),
    ("x509-examples/signed-02", "signers.0 unprotected"),
];

/// Each map re-encodes byte for byte through the order-keeping map; the typed headers hold the
/// same entries, and re-encode byte for byte exactly when the written order is theirs.
fn every_map_round_trips_and_types_alike(name: &str) {
    use cbor_static::ErrorKind;

    for (at, bytes) in maps(vector(name)) {
        assert_eq!(reencode::<HeaderMap>(&bytes).as_ref(), Ok(&bytes), "{at}");
        if TEXT_KID.contains(&(name, at.as_str())) {
            assert_eq!(
                cbor_static::from_slice::<Headers>(&bytes)
                    .map(|_| ())
                    .map_err(|e| e.kind),
                Err(ErrorKind::TypeMismatch),
                "{at}"
            );
        } else {
            same_entries(&at, &bytes);
        }
    }
}

fn same_entries(at: &str, bytes: &[u8]) {
    use std::collections::BTreeMap;

    let map: HeaderMap = cbor_static::from_slice(bytes).expect("decodes");
    let typed: Headers = cbor_static::from_slice(bytes).unwrap_or_else(|e| panic!("{at}: {e:?}"));
    assert_eq!(
        map.iter()
            .filter_map(|(label, value)| declared(label).map(|key| (key, value.as_bytes().to_vec())))
            .collect::<BTreeMap<_, _>>(),
        [
        (1, typed.alg.map(|alg| encode(&alg))),
        (2, typed.crit.as_ref().map(encode)),
        (3, typed.content_type.map(|ct| encode(&ct))),
        (4, typed.kid.map(|kid| encode(&kid))),
        (5, typed.iv.map(|iv| encode(&iv))),
        (6, typed.partial_iv.map(|piv| encode(&piv))),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key, value.expect("encodes"))))
    .collect::<BTreeMap<_, _>>(),
        "{at}: the known headers differ"
    );
    assert_eq!(
        map.iter()
            .filter(|(label, _)| declared(label).is_none())
            .map(|(label, value)| (encode(label).expect("encodes"), value.as_bytes()))
            .collect::<Vec<_>>(),
        typed
            .other
            .iter()
            .map(|entry| (entry.key.as_bytes().to_vec(), entry.value.as_bytes()))
            .collect::<Vec<_>>(),
        "{at}: the other headers differ"
    );
    assert_eq!(
        reencode::<Headers>(bytes).expect("typed") == bytes,
        map.keys()
            .map(|label| declared(label).unwrap_or(i64::MAX))
            .is_sorted(),
        "{at}: {bytes:02x?}"
    );
}

macro_rules! cases {
    ($(($test:ident, $name:literal)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                every_map_round_trips_and_types_alike($name);
            }
        )*
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/cases.rs"));

#[test]
fn an_empty_protected_header_is_an_empty_map_either_way() {
    assert_eq!(
        [b"".as_slice(), b"\xa0"].map(|bytes| {
            cbor_static_conformance::cose::Protected(bytes)
                .headers()
                .map(|map| map.len())
        }),
        [Ok(0), Ok(0)]
    );
}

#[test]
fn a_raw_header_value_keeps_its_bytes() {
    use cbor_static::RawCbor;

    let map: HeaderMap = cbor_static::from_slice(b"\xa1\x04\x42\x31\x31").expect("decodes");
    assert_eq!(
        map.get(&Label::Int(4)).map(RawCbor::as_bytes),
        Some(b"\x42\x31\x31".as_slice())
    );
}

/// RFC 9052 §3 rejects a message whose header map repeats a label. The order-keeping map does;
/// the typed headers accept a repeated label they don't name, until #79 makes the capture reject it.
#[test]
fn a_repeated_unnamed_label_passes_the_typed_headers_but_not_the_map() {
    use cbor_static::ErrorKind;

    let repeated = b"\xa2\x20\x01\x20\x02";
    assert_eq!(
        (
            cbor_static::from_slice::<HeaderMap>(repeated)
                .map(|_| ())
                .map_err(|e| e.kind),
            cbor_static::from_slice::<Headers>(repeated).map(|headers| headers.other.len()),
        ),
        (Err(ErrorKind::DuplicateKey), Ok(2))
    );
}
