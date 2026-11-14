#![forbid(unsafe_code)]
//! `flatten` (#14 Q34, R-127): a map-form struct's entries join its parent's map at the field's
//! place, concatenated at compile time.

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{decode, decode_strict, encode, golden};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Meta {
    x: u8,
    y: u8,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Parent {
    a: u8,
    #[serde(flatten)]
    meta: Meta,
    b: u8,
}

#[test]
fn a_flattened_struct_takes_its_place_in_declaration_order() {
    golden(
        &Parent {
            a: 1,
            meta: Meta { x: 2, y: 3 },
            b: 4,
        },
        b"\xa4\x61a\x01\x61x\x02\x61y\x03\x61b\x04",
    );
    assert_eq!(
        decode::<Parent>(b"\xa4\x61y\x03\x61b\x04\x61a\x01\x61x\x02"),
        Ok(Parent {
            a: 1,
            meta: Meta { x: 2, y: 3 },
            b: 4,
        })
    );
    assert_eq!(
        decode::<Parent>(b"\xa3\x61a\x01\x61x\x02\x61b\x04"),
        Err(ErrorKind::MissingField)
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Labels {
    #[cbor(key = 1)]
    alg: i8,
    #[cbor(key = -1)]
    crv: u8,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct TextOverInts {
    name: u8,
    #[cbor(flatten)]
    labels: Labels,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Names {
    name: u8,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct IntsOverText {
    #[cbor(key = 4)]
    kid: u8,
    #[cbor(flatten)]
    names: Names,
}

#[test]
fn a_parent_matches_both_kinds_of_key_its_own_and_its_childs() {
    golden(
        &TextOverInts {
            name: 9,
            labels: Labels { alg: -7, crv: 1 },
        },
        b"\xa3\x64name\x09\x01\x26\x20\x01",
    );
    golden(
        &IntsOverText {
            kid: 5,
            names: Names { name: 6 },
        },
        b"\xa2\x04\x05\x64name\x06",
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Outer {
    #[serde(flatten)]
    parent: Parent,
    z: u8,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Borrowed<'a> {
    label: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<u8>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Envelope<'a> {
    id: u8,
    #[serde(flatten)]
    body: Borrowed<'a>,
}

#[test]
fn flattening_nests_and_carries_lifetimes_and_omitted_fields() {
    golden(
        &Outer {
            parent: Parent {
                a: 1,
                meta: Meta { x: 2, y: 3 },
                b: 4,
            },
            z: 5,
        },
        b"\xa5\x61a\x01\x61x\x02\x61y\x03\x61b\x04\x61z\x05",
    );
    for (envelope, bytes) in [
        (
            Envelope {
                id: 1,
                body: Borrowed {
                    label: "hi",
                    note: None,
                },
            },
            b"\xa2\x62id\x01\x65label\x62hi".as_slice(),
        ),
        (
            Envelope {
                id: 1,
                body: Borrowed {
                    label: "hi",
                    note: Some(7),
                },
            },
            b"\xa3\x62id\x01\x65label\x62hi\x64note\x07",
        ),
    ] {
        assert_eq!(encode(&envelope), bytes);
        assert_eq!(decode::<Envelope>(bytes), Ok(envelope));
    }
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(deny_unknown_fields)]
struct Denying {
    a: u8,
    #[serde(flatten)]
    meta: Meta,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(deny_unknown_fields)]
struct StrictMeta {
    x: u8,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Skipping {
    a: u8,
    #[serde(flatten)]
    meta: StrictMeta,
}

#[test]
fn the_parent_alone_decides_what_an_unknown_entry_does() {
    assert_eq!(
        decode::<Denying>(b"\xa4\x61a\x01\x61x\x02\x61y\x03\x61q\x04"),
        Err(ErrorKind::UnknownField)
    );
    assert_eq!(
        decode::<Denying>(b"\xa3\x61a\x01\x61x\x02\x61y\x03"),
        Ok(Denying {
            a: 1,
            meta: Meta { x: 2, y: 3 },
        })
    );
    assert_eq!(
        decode::<Skipping>(b"\xa3\x61a\x01\x61q\x04\x61x\x02"),
        Ok(Skipping {
            a: 1,
            meta: StrictMeta { x: 2 },
        })
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct Inner {
    c: u8,
    a: u8,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct Sorted {
    d: u8,
    #[cbor(flatten)]
    inner: Inner,
    b: u8,
}

#[test]
fn a_deterministic_parent_sorts_its_childs_keys_among_its_own() {
    let sorted = Sorted {
        d: 4,
        inner: Inner { c: 3, a: 1 },
        b: 2,
    };
    golden(&sorted, b"\xa4\x61a\x01\x61b\x02\x61c\x03\x61d\x04");
    assert_eq!(decode_strict::<Sorted>(&encode(&sorted)), Ok(sorted));
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy, Default)]
struct HalfOfMany {
    b0: u8,
    b1: u8,
    b2: u8,
    b3: u8,
    b4: u8,
    b5: u8,
    b6: u8,
    b7: u8,
    b8: u8,
    b9: u8,
    b10: u8,
    b11: u8,
    b12: u8,
    b13: u8,
    b14: u8,
    b15: u8,
    b16: u8,
    b17: u8,
    b18: u8,
    b19: u8,
    b20: u8,
    b21: u8,
    b22: u8,
    b23: u8,
    b24: u8,
    b25: u8,
    b26: u8,
    b27: u8,
    b28: u8,
    b29: u8,
    b30: u8,
    b31: u8,
}

/// 64 entries, the most a map holds, half of them flattened.
#[derive(Cbor, Debug, PartialEq, Clone, Copy, Default)]
struct Many {
    a0: u8,
    a1: u8,
    a2: u8,
    a3: u8,
    a4: u8,
    a5: u8,
    a6: u8,
    a7: u8,
    a8: u8,
    a9: u8,
    a10: u8,
    a11: u8,
    a12: u8,
    a13: u8,
    a14: u8,
    a15: u8,
    a16: u8,
    a17: u8,
    a18: u8,
    a19: u8,
    a20: u8,
    a21: u8,
    a22: u8,
    a23: u8,
    a24: u8,
    a25: u8,
    a26: u8,
    a27: u8,
    a28: u8,
    a29: u8,
    a30: u8,
    a31: u8,
    #[serde(flatten)]
    half: HalfOfMany,
}

#[test]
fn a_struct_holds_64_entries_its_flattened_ones_included() {
    let many = Many {
        a31: 7,
        half: HalfOfMany {
            b31: 9,
            ..HalfOfMany::default()
        },
        ..Many::default()
    };
    let bytes = encode(&many);
    assert_eq!(bytes[..2], *b"\xb8\x40");
    assert_eq!(bytes[bytes.len() - 5..], *b"\x63b31\x09");
    assert_eq!(decode::<Many>(&bytes), Ok(many));
}
