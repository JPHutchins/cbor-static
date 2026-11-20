#![cfg(feature = "serde")]
//! `deserialize_any` hands each item to the visitor method its own type names (#6 Q7): integers
//! and bignums of up to 64 bits as `u64` or `i64` (#6 Q19), floats as `f64`, null as unit (#6 Q21),
//! strings borrowed, and arrays and maps entered one level deeper (R-303).

use cbor_static::ErrorKind;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// What `deserialize_any` handed over.
#[derive(Debug, PartialEq)]
enum Seen<'de> {
    Unsigned(u64),
    Signed(i64),
    Float(u64),
    Bool(bool),
    Unit,
    Text(&'de str),
    Bytes(&'de [u8]),
    OwnedText(String),
    OwnedBytes(Vec<u8>),
    Array(Vec<Self>),
    Map(Vec<(Self, Self)>),
}

impl<'de> Deserialize<'de> for Seen<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Recorder)
    }
}

struct Recorder;

impl<'de> Visitor<'de> for Recorder {
    type Value = Seen<'de>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("anything")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Seen<'de>, E> {
        Ok(Seen::Unsigned(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Seen<'de>, E> {
        Ok(Seen::Signed(v))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Seen<'de>, E> {
        Ok(Seen::Float(v.to_bits()))
    }

    fn visit_bool<E>(self, v: bool) -> Result<Seen<'de>, E> {
        Ok(Seen::Bool(v))
    }

    fn visit_unit<E>(self) -> Result<Seen<'de>, E> {
        Ok(Seen::Unit)
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Seen<'de>, E> {
        Ok(Seen::Text(v))
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Seen<'de>, E> {
        Ok(Seen::Bytes(v))
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Seen<'de>, E> {
        Err(E::custom("a string neither borrowed nor owned"))
    }

    fn visit_string<E>(self, v: String) -> Result<Seen<'de>, E> {
        Ok(Seen::OwnedText(v))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Seen<'de>, E> {
        Ok(Seen::OwnedBytes(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Seen<'de>, A::Error> {
        core::iter::from_fn(|| seq.next_element().transpose())
            .collect::<Result<_, _>>()
            .map(Seen::Array)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Seen<'de>, A::Error> {
        core::iter::from_fn(|| map.next_entry().transpose())
            .collect::<Result<_, _>>()
            .map(Seen::Map)
    }
}

fn seen(bytes: &[u8]) -> Result<Seen<'_>, (ErrorKind, u32)> {
    cbor_static::serde::from_slice(bytes).map_err(|e| (e.kind, e.offset))
}

#[test]
fn integers_are_u64_or_i64() {
    assert_eq!(seen(b"\x00"), Ok(Seen::Unsigned(0)));
    assert_eq!(
        seen(b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff"),
        Ok(Seen::Unsigned(u64::MAX))
    );
    assert_eq!(seen(b"\x20"), Ok(Seen::Signed(-1)));
    assert_eq!(
        seen(b"\x3b\x7f\xff\xff\xff\xff\xff\xff\xff"),
        Ok(Seen::Signed(i64::MIN))
    );
}

/// serde's default `visit_u128` and `visit_i128` format their error with `core::fmt`, and its
/// buffering visitor keeps them, so `deserialize_any` hands on only values of 64 bits (#6 Q19).
#[test]
fn integers_beyond_64_bits_are_refused_past_them() {
    let negative = b"\x3b\x80\x00\x00\x00\x00\x00\x00\x00";
    assert_eq!(seen(negative), Err((ErrorKind::TypeMismatch, 9)));
    assert_eq!(
        cbor_static::serde::from_slice::<i128>(negative),
        Ok(i128::from(i64::MIN) - 1)
    );
    let two_to_64 = b"\xc2\x49\x01\x00\x00\x00\x00\x00\x00\x00\x00";
    assert_eq!(seen(two_to_64), Err((ErrorKind::TypeMismatch, 11)));
    assert_eq!(
        cbor_static::serde::from_slice::<u128>(two_to_64),
        Ok(1 << 64)
    );
    assert_eq!(
        seen(b"\xc3\x49\x01\x00\x00\x00\x00\x00\x00\x00\x00"),
        Err((ErrorKind::TypeMismatch, 11))
    );
    assert_eq!(
        seen(b"\xc3\x48\x80\x00\x00\x00\x00\x00\x00\x00"),
        Err((ErrorKind::TypeMismatch, 10))
    );
}

#[test]
fn bignums_of_64_bits_are_u64_or_i64() {
    assert_eq!(seen(b"\xc2\x41\x05"), Ok(Seen::Unsigned(5)));
    assert_eq!(seen(b"\xc3\x41\x05"), Ok(Seen::Signed(-6)));
    assert_eq!(
        seen(b"\xc2\x48\xff\xff\xff\xff\xff\xff\xff\xff"),
        Ok(Seen::Unsigned(u64::MAX))
    );
    assert_eq!(
        seen(b"\xc3\x48\x7f\xff\xff\xff\xff\xff\xff\xff"),
        Ok(Seen::Signed(i64::MIN))
    );
}

#[test]
fn floats_are_f64_bools_bools_and_null_unit() {
    assert_eq!(seen(b"\xf9\x3c\x00"), Ok(Seen::Float(1.0f64.to_bits())));
    assert_eq!(
        seen(b"\xfa\x3f\xc0\x00\x00"),
        Ok(Seen::Float(1.5f64.to_bits()))
    );
    assert_eq!(
        seen(b"\xfb\x3f\xb9\x99\x99\x99\x99\x99\x9a"),
        Ok(Seen::Float(0.1f64.to_bits()))
    );
    assert_eq!(seen(b"\xf5"), Ok(Seen::Bool(true)));
    assert_eq!(seen(b"\xf4"), Ok(Seen::Bool(false)));
    assert_eq!(seen(b"\xf6"), Ok(Seen::Unit));
}

#[test]
fn definite_strings_are_borrowed() {
    assert_eq!(seen(b"\x62ab"), Ok(Seen::Text("ab")));
    assert_eq!(seen(b"\x42\x01\x02"), Ok(Seen::Bytes(&[1, 2])));
}

#[cfg(feature = "alloc")]
#[test]
fn strings_in_chunks_are_owned() {
    assert_eq!(
        seen(b"\x7f\x61a\x61b\xff"),
        Ok(Seen::OwnedText("ab".into()))
    );
    assert_eq!(seen(b"\x5f\x41\x01\xff"), Ok(Seen::OwnedBytes(vec![1])));
}

#[test]
fn arrays_and_maps_are_entered() {
    assert_eq!(
        seen(b"\x82\x01\x9f\x20\xff"),
        Ok(Seen::Array(vec![
            Seen::Unsigned(1),
            Seen::Array(vec![Seen::Signed(-1)])
        ]))
    );
    assert_eq!(
        seen(b"\xbf\x61a\x01\x02\xf6\xff"),
        Ok(Seen::Map(vec![
            (Seen::Text("a"), Seen::Unsigned(1)),
            (Seen::Unsigned(2), Seen::Unit)
        ]))
    );
    let deep = [[0x81; 33].as_slice(), &[0x00]].concat();
    assert_eq!(seen(&deep), Err((ErrorKind::LimitExceeded, 32)));
    assert!(seen(&deep[1..]).is_ok());
}

/// The native analogue of first-byte dispatch.
#[derive(cbor_static::Decode, Debug)]
#[serde(untagged)]
enum Dispatched<'a> {
    Number(u64),
    Name(&'a str),
}

#[test]
fn other_tags_and_simple_values_are_refused_as_native_dispatch_refuses_them() {
    assert!(matches!(
        cbor_static::from_slice::<Dispatched>(b"\x61a"),
        Ok(Dispatched::Name("a"))
    ));
    assert!(matches!(
        cbor_static::from_slice::<Dispatched>(b"\x01"),
        Ok(Dispatched::Number(1))
    ));
    for bytes in [
        &b"\xc1\x01"[..],
        b"\xd8\x20\x61a",
        b"\xf7",
        b"\xf0",
        b"\xf8\x20",
    ] {
        assert_eq!(
            seen(bytes).map(|_| ()),
            cbor_static::from_slice::<Dispatched>(bytes)
                .map(|_| ())
                .map_err(|e| (e.kind, e.offset)),
            "{bytes:02x?}"
        );
        assert_eq!(
            seen(bytes).map_err(|(kind, _)| kind),
            Err(ErrorKind::TypeMismatch)
        );
    }
}

#[test]
fn malformed_items_are_refused_where_the_tokens_refuse_them() {
    assert_eq!(seen(b"\xff"), Err((ErrorKind::NotWellFormed, 1)));
    assert_eq!(seen(b"\x18"), Err((ErrorKind::UnexpectedEnd, 1)));
    assert_eq!(seen(b"\x82\x01"), Err((ErrorKind::UnexpectedEnd, 2)));
}

#[cfg(feature = "alloc")]
mod buffered {
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(untagged)]
    enum Id {
        Number(u64),
        Name(String),
        Pair(u8, u8),
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(tag = "type")]
    enum Shape {
        Circle { r: u32 },
        Empty,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct Inner {
        x: u8,
        y: u8,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct Outer {
        a: u8,
        #[serde(flatten)]
        inner: Inner,
    }

    #[test]
    fn serde_buffered_forms_decode_through_any_under_alloc() {
        assert_eq!(
            cbor_static::serde::from_slice::<Id>(b"\x07"),
            Ok(Id::Number(7))
        );
        assert_eq!(
            cbor_static::serde::from_slice::<Id>(b"\x7f\x61a\xff"),
            Ok(Id::Name("a".into()))
        );
        assert_eq!(
            cbor_static::serde::from_slice::<Id>(b"\x82\x01\x02"),
            Ok(Id::Pair(1, 2))
        );
        assert_eq!(
            cbor_static::serde::from_slice::<Shape>(b"\xa2\x64type\x66Circle\x61r\x02"),
            Ok(Shape::Circle { r: 2 })
        );
        assert_eq!(
            cbor_static::serde::from_slice::<Shape>(b"\xa1\x64type\x65Empty"),
            Ok(Shape::Empty)
        );
        assert_eq!(
            cbor_static::serde::from_slice::<Outer>(b"\xa3\x61y\x03\x61a\x01\x61x\x02"),
            Ok(Outer {
                a: 1,
                inner: Inner { x: 2, y: 3 }
            })
        );
    }

    #[derive(serde::Serialize, Deserialize, Debug, PartialEq)]
    struct Marker;

    #[derive(serde::Serialize, Deserialize, Debug, PartialEq)]
    #[serde(untagged)]
    enum WithUnits {
        Units { m: Marker, u: (), o: Option<u8> },
    }

    #[derive(serde::Serialize, Deserialize, Debug, PartialEq)]
    struct Units {
        m: Marker,
        u: (),
        o: Option<u8>,
    }

    #[derive(serde::Serialize, Deserialize, Debug, PartialEq)]
    struct Flattened {
        a: u8,
        #[serde(flatten)]
        units: Units,
    }

    /// Null is both layers' unit, `None` included, so buffered forms read back their own (#6 Q21).
    #[test]
    fn units_and_none_inside_buffered_forms_round_trip() {
        for o in [None, Some(3)] {
            let untagged = WithUnits::Units {
                m: Marker,
                u: (),
                o,
            };
            let bytes = cbor_static::serde::to_vec(&untagged).expect("encodes");
            assert_eq!(
                cbor_static::serde::from_slice::<WithUnits>(&bytes),
                Ok(untagged)
            );
            let flattened = Flattened {
                a: 1,
                units: Units {
                    m: Marker,
                    u: (),
                    o,
                },
            };
            let bytes = cbor_static::serde::to_vec(&flattened).expect("encodes");
            assert_eq!(
                cbor_static::serde::from_slice::<Flattened>(&bytes),
                Ok(flattened)
            );
        }
    }

    #[test]
    fn a_buffered_integer_beyond_64_bits_is_refused() {
        assert_eq!(
            cbor_static::serde::from_slice::<Id>(b"\xc2\x49\x01\x00\x00\x00\x00\x00\x00\x00\x00")
                .map_err(|e| (e.kind, e.offset)),
            Err((cbor_static::ErrorKind::TypeMismatch, 11))
        );
    }

    #[test]
    fn buffering_is_bounded_by_the_depth_limit() {
        let deep = [[0x81; 40].as_slice(), &[0x00]].concat();
        assert_eq!(
            cbor_static::serde::from_slice::<Id>(&deep).map_err(|e| (e.kind, e.offset)),
            Err((cbor_static::ErrorKind::LimitExceeded, 32))
        );
    }
}
