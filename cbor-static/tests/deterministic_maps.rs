//! Deterministic `LinearMap`s (D-11b, Q33): entries sorted as they are written, with no scratch
//! space, by the root's ops, in RFC 8949 §4.2.1's bytewise order and §4.2.3's length-first order.
#![cfg(feature = "heapless")]
#![forbid(unsafe_code)]

mod common;

use core::cmp::Ordering;

use cbor_static::{Cbor, Encode, ErrorKind, F16, encode_into};
use common::{chunked_encode, encode};
use heapless::LinearMap;
use proptest::prelude::*;

/// RFC 8949's example keys, each kind an untagged variant.
#[derive(Encode, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(untagged)]
enum Key {
    Int(i64),
    Text(&'static str),
    List([i64; 1]),
    Flag(bool),
}

/// The keys of RFC 8949 §4.2.1's example in its order.
const RFC_8949_BYTEWISE: [Key; 8] = [
    Key::Int(10),
    Key::Int(100),
    Key::Int(-1),
    Key::Text("z"),
    Key::Text("aa"),
    Key::List([100]),
    Key::List([-1]),
    Key::Flag(false),
];

/// The map from each of [`RFC_8949_BYTEWISE`]'s keys to its place, as §4.2.1 sorts it.
const BYTEWISE: &[u8] =
    b"\xa8\x0a\x00\x18\x64\x01\x20\x02\x61z\x03\x62aa\x04\x81\x18\x64\x05\x81\x20\x06\xf4\x07";

/// The same map as §4.2.3 sorts it.
const LENGTH_FIRST: &[u8] =
    b"\xa8\x0a\x00\x20\x02\xf4\x07\x18\x64\x01\x61z\x03\x81\x20\x06\x62aa\x04\x81\x18\x64\x05";

#[derive(Encode, Debug, PartialEq)]
#[cbor(deterministic)]
struct SortedMap(LinearMap<Key, u8, 8>);

#[derive(Encode, Debug, PartialEq)]
#[cbor(length_first)]
struct LengthFirstMap(LinearMap<Key, u8, 8>);

/// Each key maps to its place in §4.2.1's order, inserted in `insertion` order.
fn map(insertion: impl Iterator<Item = usize>) -> LinearMap<Key, u8, 8> {
    insertion
        .map(|at| (RFC_8949_BYTEWISE[at], u8::try_from(at).expect("eight keys")))
        .collect()
}

#[test]
fn a_linear_map_sorts_as_rfc_8949_lists_its_example_keys() {
    for insertion in [
        map(0..8),
        map((0..8).rev()),
        map([0, 7, 1, 6, 2, 5, 3, 4].into_iter()),
        map([1, 6, 3, 0, 7, 2, 5, 4].into_iter()),
    ] {
        let (sorted, length_firsts) = (SortedMap(insertion.clone()), LengthFirstMap(insertion));
        for chunk in (1..=24).step_by(if cfg!(miri) { 7 } else { 1 }) {
            assert_eq!(
                chunked_encode(&sorted, chunk),
                BYTEWISE,
                "chunks of {chunk}"
            );
            assert_eq!(
                chunked_encode(&length_firsts, chunk),
                LENGTH_FIRST,
                "chunks of {chunk}"
            );
        }
    }
}

#[derive(Encode, Debug, PartialEq)]
#[serde(transparent)]
#[cbor(deterministic)]
struct TransparentSorted {
    map: LinearMap<Key, u8, 8>,
}

#[derive(Encode, Debug, PartialEq)]
#[serde(transparent)]
#[cbor(length_first)]
struct TransparentLengthFirst {
    map: LinearMap<Key, u8, 8>,
}

#[test]
fn a_transparent_struct_is_a_deterministic_root_as_a_newtype_is() {
    let reversed = map((0..8).rev());
    assert_eq!(
        encode(&TransparentSorted {
            map: reversed.clone()
        }),
        BYTEWISE
    );
    assert_eq!(
        encode(&TransparentLengthFirst { map: reversed }),
        LENGTH_FIRST
    );
}

#[derive(Cbor, Debug, PartialEq)]
#[cbor(deterministic)]
struct Halves(LinearMap<F16, u8, 4>);

#[test]
fn keys_that_encode_alike_cannot_be_sorted() {
    let halves = Halves(
        [(F16::from_bits(0x7e01), 1), (F16::from_bits(0x7e02), 2)]
            .into_iter()
            .collect(),
    );
    assert_eq!(
        encode_into(&halves, &mut [0; 16]).map_err(|error| error.kind),
        Err(ErrorKind::DuplicateKey)
    );
}

/// The encoding of `entries` as a map whose keys sort by `order` over their encodings.
fn expected(entries: &[(i32, u8)], order: fn(&[u8], &[u8]) -> Ordering) -> Vec<u8> {
    let mut encoded: Vec<(Vec<u8>, Vec<u8>)> = entries
        .iter()
        .map(|(key, value)| (encode(key), encode(value)))
        .collect();
    encoded.sort_by(|(a, _), (b, _)| order(a, b));
    core::iter::once(vec![
        0xa0 + u8::try_from(entries.len()).expect("at most 16 entries"),
    ])
    .chain(encoded.into_iter().flat_map(|(key, value)| [key, value]))
    .flatten()
    .collect()
}

#[derive(Cbor, Debug, PartialEq)]
#[cbor(deterministic)]
struct Ints(LinearMap<i32, u8, 16>);

#[derive(Cbor, Debug, PartialEq)]
#[cbor(length_first)]
struct IntsLengthFirst(LinearMap<i32, u8, 16>);

proptest! {
    #![proptest_config(ProptestConfig {
        cases: if cfg!(miri) { 2 } else { 256 },
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_insertion_order_sorts_to_one_encoding(
        entries in proptest::collection::hash_map(any::<i32>(), any::<u8>(), 0..=16)
            .prop_map(|entries| entries.into_iter().collect::<Vec<_>>())
            .prop_shuffle(),
        chunk in 1usize..=8,
    ) {
        let map: LinearMap<i32, u8, 16> = entries.iter().copied().collect();
        let bytewise = expected(&entries, Ord::cmp);
        let length_first = expected(&entries, |a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        prop_assert_eq!(encode(&Ints(map.clone())), bytewise.clone());
        prop_assert_eq!(chunked_encode(&Ints(map.clone()), chunk), bytewise);
        prop_assert_eq!(encode(&IntsLengthFirst(map.clone())), length_first.clone());
        prop_assert_eq!(chunked_encode(&IntsLengthFirst(map), chunk), length_first);
    }
}
