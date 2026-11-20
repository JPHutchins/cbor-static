#![cfg(all(feature = "serde", feature = "alloc"))]
//! R-304: the internally tagged, adjacently tagged and untagged forms, in `serde_derive`'s bytes,
//! through both layers. serde decodes these forms by buffering the item and dispatching with its own
//! code (D-10c), so where the two layers decode alike both are checked, and each known divergence
//! is pinned at serde's result.

use core::fmt::Debug;

use cbor_static::{Decode, Encode, ErrorKind};
use serde::{Deserialize, Serialize};

/// `value` is `bytes` through both layers, and `bytes` decodes to `value` through both.
fn golden<'de, T>(value: &T, bytes: &'de [u8])
where
    T: Serialize + Encode + Deserialize<'de> + Decode<'de> + PartialEq + Debug,
{
    assert_eq!(
        cbor_static::to_vec(value).as_deref(),
        Ok(bytes),
        "native encode"
    );
    assert_eq!(
        cbor_static::serde::to_vec(value).as_deref(),
        Ok(bytes),
        "serde encode"
    );
    assert_eq!(
        cbor_static::from_slice::<T>(bytes).as_ref(),
        Ok(value),
        "native decode"
    );
    assert_eq!(
        cbor_static::serde::from_slice::<T>(bytes).as_ref(),
        Ok(value),
        "serde decode"
    );
}

fn kinds<'de, T: Deserialize<'de> + Decode<'de> + Debug>(
    bytes: &'de [u8],
) -> (Result<T, ErrorKind>, Result<T, ErrorKind>) {
    (
        cbor_static::from_slice::<T>(bytes).map_err(|e| e.kind),
        cbor_static::serde::from_slice::<T>(bytes).map_err(|e| e.kind),
    )
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(tag = "type")]
enum Internal {
    Circle { r: u32 },
    Empty,
}

#[test]
fn internally_tagged() {
    golden(
        &Internal::Circle { r: 2 },
        b"\xa2\x64type\x66Circle\x61r\x02",
    );
    golden(&Internal::Empty, b"\xa1\x64type\x65Empty");
    assert_eq!(
        kinds::<Internal>(b"\xa2\x61r\x02\x64type\x66Circle"),
        (Ok(Internal::Circle { r: 2 }), Ok(Internal::Circle { r: 2 })),
        "the tag after the fields"
    );
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Level(u32),
    Span { from: i8 },
    Idle,
}

#[test]
fn adjacently_tagged() {
    golden(&Adjacent::Level(7), b"\xa2\x61t\x65Level\x61c\x07");
    golden(
        &Adjacent::Span { from: -1 },
        b"\xa2\x61t\x64Span\x61c\xa1\x64from\x20",
    );
    golden(&Adjacent::Idle, b"\xa1\x61t\x64Idle");
    assert_eq!(
        kinds::<Adjacent>(b"\xa2\x61c\x07\x61t\x65Level"),
        (Ok(Adjacent::Level(7)), Ok(Adjacent::Level(7))),
        "content before tag, which serde buffers"
    );
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(untagged)]
enum Untagged {
    Number(u64),
    Name(String),
    Flags([bool; 2]),
}

#[test]
fn untagged() {
    golden(&Untagged::Number(500), b"\x19\x01\xf4");
    golden(&Untagged::Name("ab".into()), b"\x62ab");
    golden(&Untagged::Flags([true, false]), b"\x82\xf5\xf4");
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(untagged)]
enum Numeric {
    Float(f64),
    Int(u32),
}

/// D-10c: serde hands a buffered integer to a float, so the first variant wins.
#[test]
fn divergence_an_integer_decodes_into_a_float_through_serde() {
    assert_eq!(
        cbor_static::to_vec(&Numeric::Int(7)).as_deref(),
        Ok(&b"\x07"[..])
    );
    assert_eq!(
        cbor_static::serde::to_vec(&Numeric::Int(7)).as_deref(),
        Ok(&b"\x07"[..])
    );
    assert_eq!(
        cbor_static::from_slice::<Numeric>(b"\x07"),
        Ok(Numeric::Int(7))
    );
    assert_eq!(
        cbor_static::serde::from_slice::<Numeric>(b"\x07"),
        Ok(Numeric::Float(7.0))
    );
    assert_eq!(
        cbor_static::serde::to_vec(&Numeric::Float(7.0)).as_deref(),
        Ok(&b"\xf9\x47\x00"[..])
    );
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(tag = "t")]
enum Labelled {
    Text { s: String },
}

/// D-10c: serde hands a buffered byte string to a string.
#[test]
fn divergence_a_byte_string_decodes_into_a_string_through_serde() {
    let bytes = b"\xa2\x61t\x64Text\x61s\x41a";
    let (native, serde) = kinds::<Labelled>(bytes);
    assert_eq!(native, Err(ErrorKind::TypeMismatch));
    assert_eq!(serde, Ok(Labelled::Text { s: "a".into() }));
}

/// D-10c: serde decodes a buffered struct's entries itself, so Strict never sees one go unclaimed.
#[test]
fn divergence_strict_finds_no_unknown_field_inside_a_buffered_form() {
    let sorted = b"\xa3\x61r\x02\x61z\x00\x64type\x66Circle";
    let mut slot = core::mem::MaybeUninit::uninit();
    assert_eq!(
        cbor_static::decode_strict_into::<Internal>(sorted, &mut slot)
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ErrorKind::UnknownField)
    );
    assert_eq!(
        cbor_static::serde::Deserializer::new_strict(sorted).decode::<Internal>(),
        Ok(Internal::Circle { r: 2 })
    );
}

/// serde-rs/serde#2172: buffered content reports itself human-readable, whatever the format, so a
/// type with two representations reads the other one inside a buffered form.
#[test]
fn divergence_buffered_content_is_human_readable() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    #[serde(untagged)]
    enum Peer {
        Address(core::net::Ipv4Addr),
    }

    let address = core::net::Ipv4Addr::new(192, 0, 2, 1);
    let alone = cbor_static::serde::to_vec(&address).expect("encodes");
    assert_eq!(alone, b"\x84\x18\xc0\x00\x02\x01");
    assert_eq!(cbor_static::serde::from_slice(&alone), Ok(address));
    assert_eq!(
        cbor_static::serde::to_vec(&Peer::Address(address)).as_deref(),
        Ok(&alone[..])
    );
    assert_eq!(
        cbor_static::serde::from_slice::<Peer>(&alone).map_err(|e| e.kind),
        Err(ErrorKind::Custom)
    );
}

/// A plain `&[u8]` is a byte string natively, but serde writes it as an array of integers and reads
/// it only from a byte string; `serde_bytes` makes the two agree (D-08a).
#[test]
fn divergence_a_plain_byte_slice_is_an_array_through_serde() {
    #[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
    struct Plain<'a> {
        #[serde(borrow)]
        b: &'a [u8],
    }

    let value = Plain { b: &[1, 2] };
    assert_eq!(
        cbor_static::to_vec(&value).as_deref(),
        Ok(&b"\xa1\x61b\x42\x01\x02"[..])
    );
    let through_serde = cbor_static::serde::to_vec(&value).expect("encodes");
    assert_eq!(through_serde, b"\xa1\x61b\x82\x01\x02");
    assert_eq!(
        cbor_static::serde::from_slice::<Plain>(b"\xa1\x61b\x42\x01\x02"),
        Ok(Plain { b: &[1, 2] })
    );
    assert_eq!(
        cbor_static::serde::from_slice::<Plain>(&through_serde).map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
}
