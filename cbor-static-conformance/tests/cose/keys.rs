//! `COSE_Key` and `COSE_KeySet`, from the ephemeral and static keys the examples carry in their
//! headers (RFC 9053 §6.3.1's ephemeral key -1 and static key -2), one case per example. RFC 9052
//! Appendix C.7's key sets are diagnostic notation only, so they are not vectors here.

use cbor_static_conformance::cose::{Key, KeySet, Label};

use crate::common::vectors::{COSE_WG, Kind, Vector, vector};
use crate::common::{encode, reencode};
use crate::decoded::{intact, layers};

/// The keys in an example's headers, as written; none where the generator broke the tag.
pub fn keys(vector: &Vector<Kind>) -> Vec<(String, &'static [u8])> {
    intact(vector).map_or_else(Vec::new, |decoded| {
        layers(&decoded)
            .into_iter()
            .flat_map(|layer| {
                [-1, -2].into_iter().filter_map(move |label| {
                    layer
                        .unprotected
                        .get(&Label::Int(label))
                        .map(|raw| (format!("{at} {label}", at = layer.at), raw.as_bytes()))
                })
            })
            .collect()
    })
}

/// The typed struct's field order: RFC 9052 §7's common parameters as declared, then the key
/// type's own.
fn declared(label: &Label) -> Option<i64> {
    match label {
        Label::Int(common @ 1..=5) => Some(*common),
        Label::Int(_) | Label::Text(_) => None,
    }
}

/// Each key decodes with its entries as written, compared as data, and re-encodes byte for byte
/// exactly when the written order is the typed struct's; the example's keys as a set round-trip.
fn every_key_decodes_as_written(name: &str) {
    let keys = keys(vector(name));
    for (at, bytes) in &keys {
        same_entries(at, bytes);
    }
    let set = key_set(&keys);
    let written = encode(&set).expect("encodes");
    assert_eq!(
        cbor_static::from_slice::<KeySet>(&written).as_ref(),
        Ok(&set)
    );
}

/// The keys as one `COSE_KeySet`, as the types write it.
pub fn key_set(keys: &[(String, &'static [u8])]) -> KeySet<'static> {
    KeySet(
        keys.iter()
            .map(|(_, bytes)| cbor_static::from_slice::<Key>(bytes).expect("decodes"))
            .collect(),
    )
}

fn same_entries(at: &str, bytes: &[u8]) {
    use std::collections::BTreeMap;

    use cbor_static::RawCbor;
    use heapless::LinearMap;

    let written: LinearMap<Label, RawCbor, 8> = cbor_static::from_slice(bytes).expect("a map");
    let key: Key = cbor_static::from_slice(bytes).unwrap_or_else(|e| panic!("{at}: {e:?}"));
    assert_eq!(
        written
            .iter()
            .filter_map(
                |(label, value)| declared(label).map(|label| (label, value.as_bytes().to_vec()))
            )
            .collect::<BTreeMap<_, _>>(),
        [
            (1, Some(encode(&key.kty))),
            (2, key.kid.map(|kid| encode(&kid))),
            (3, key.alg.map(|alg| encode(&alg))),
            (4, key.key_ops.as_ref().map(encode)),
            (5, key.base_iv.map(|iv| encode(&iv))),
        ]
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| (label, value.expect("encodes"))))
        .collect::<BTreeMap<_, _>>(),
        "{at}"
    );
    assert_eq!(
        written
            .iter()
            .filter(|(label, _)| declared(label).is_none())
            .map(|(label, value)| (encode(label).expect("encodes"), value.as_bytes()))
            .collect::<Vec<_>>(),
        key.parameters
            .iter()
            .map(|entry| (entry.key.as_bytes().to_vec(), entry.value.as_bytes()))
            .collect::<Vec<_>>(),
        "{at}"
    );
    assert_eq!(
        reencode::<Key>(bytes).expect("encodes") == bytes,
        written
            .keys()
            .map(|label| declared(label).unwrap_or(i64::MAX))
            .is_sorted(),
        "{at}"
    );
}

macro_rules! cases {
    ($(($test:ident, $name:literal)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                every_key_decodes_as_written($name);
            }
        )*
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/cases.rs"));

#[test]
fn the_examples_carry_ephemeral_and_static_keys() {
    let labels = COSE_WG
        .iter()
        .flat_map(keys)
        .map(|(at, _)| at.rsplit_once(' ').map(|(_, label)| label.to_owned()))
        .collect::<Vec<_>>();
    assert_eq!(
        ["-1", "-2"].map(|label| labels
            .iter()
            .filter(|at| at.as_deref() == Some(label))
            .count()),
        [35, 30]
    );
}

/// RFC 9052 §3's unique labels apply to a key's map too; a key accepts a repeated label it doesn't
/// name, until #79 makes the capture reject it.
#[test]
fn a_repeated_unnamed_label_passes_the_key() {
    assert_eq!(
        cbor_static::from_slice::<Key>(b"\xa3\x01\x01\x20\x01\x20\x02")
            .map(|key| key.parameters.len()),
        Ok(2)
    );
}
