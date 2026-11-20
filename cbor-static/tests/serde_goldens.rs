#![cfg(all(feature = "serde", feature = "alloc"))]
//! The serde layer writes the native derive's bytes for every shape of serde's data model (D-08),
//! and reads them back, through both entry points of each direction.

use core::fmt::Debug;

use cbor_static::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// `value` is `bytes` through both layers, and `bytes` decodes to `value` through both.
fn alike<'a, T>(value: &T, bytes: &'a [u8])
where
    T: Serialize + Deserialize<'a> + Encode + Decode<'a> + PartialEq + Debug,
{
    let mut native = [0u8; 64];
    let len = cbor_static::to_slice(value, &mut native).expect("native encode");
    assert_eq!(&native[..len], bytes, "native encode");
    serde_only(value, bytes);
    assert_eq!(
        cbor_static::from_slice::<T>(bytes).as_ref(),
        Ok(value),
        "native decode"
    );
}

/// `value` is `bytes` through the serde layer, both ways.
fn serde_only<'a, T>(value: &T, bytes: &'a [u8])
where
    T: Serialize + Deserialize<'a> + PartialEq + Debug,
{
    let mut out = [0u8; 64];
    let len = cbor_static::serde::to_slice(value, &mut out).expect("serde encode");
    assert_eq!(&out[..len], bytes, "serde encode");
    assert_eq!(
        cbor_static::serde::to_vec(value).as_deref(),
        Ok(bytes),
        "serde to_vec"
    );
    assert_eq!(
        cbor_static::serde::from_slice::<T>(bytes).as_ref(),
        Ok(value),
        "serde decode"
    );
    assert_eq!(
        cbor_static::serde::from_slice_partial::<T>(bytes),
        Ok((
            cbor_static::serde::from_slice::<T>(bytes).expect("serde decode"),
            &[][..]
        )),
        "serde partial decode"
    );
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Reading<'a> {
    id: u32,
    name: &'a str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    rssi: Option<i32>,
    last: Option<u8>,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Ping;

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Meters(u32);

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Span(u32, Option<u32>);

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
enum Shape {
    Empty,
    Square(u32),
    Circle { r: u32, filled: bool },
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Outer {
    inner: Meters,
    shapes: [Shape; 2],
}

#[test]
fn structs_are_maps_of_text_keys_in_declaration_order() {
    alike(
        &Reading {
            id: 7,
            name: "a",
            ok: true,
            rssi: None,
            last: None,
        },
        b"\xa4\x62id\x07\x64name\x61a\x62ok\xf5\x64last\xf6",
    );
    alike(
        &Reading {
            id: 7,
            name: "a",
            ok: false,
            rssi: Some(-1),
            last: Some(2),
        },
        b"\xa5\x62id\x07\x64name\x61a\x62ok\xf4\x64rssi\x20\x64last\x02",
    );
}

#[test]
fn unit_shapes_are_null_and_a_newtype_is_its_field() {
    alike(&Ping, b"\xf6");
    alike(&(), b"\xf6");
    alike(&None::<u32>, b"\xf6");
    alike(&Some(1u32), b"\x01");
    alike(&Meters(1000), b"\x19\x03\xe8");
}

#[test]
fn tuples_and_arrays_are_arrays() {
    alike(&Span(1, None), b"\x82\x01\xf6");
    alike(&(1u8, -2i16, "x"), b"\x83\x01\x21\x61x");
    alike(&[1u8, 2, 3], b"\x83\x01\x02\x03");
    alike(&[0u16; 0], b"\x80");
    alike(&vec![1u8, 0xff], b"\x82\x01\x18\xff");
}

#[test]
fn variants_are_externally_tagged() {
    alike(&Shape::Empty, b"\x65Empty");
    alike(&Shape::Square(2), b"\xa1\x66Square\x02");
    alike(
        &Shape::Circle { r: 1, filled: true },
        b"\xa1\x66Circle\xa2\x61r\x01\x66filled\xf5",
    );
    alike(
        &Outer {
            inner: Meters(3),
            shapes: [Shape::Empty, Shape::Square(4)],
        },
        b"\xa2\x65inner\x03\x66shapes\x82\x65Empty\xa1\x66Square\x04",
    );
}

#[test]
fn integers_take_their_shortest_head() {
    alike(&0u8, b"\x00");
    alike(&23u8, b"\x17");
    alike(&24u8, b"\x18\x18");
    alike(&u16::MAX, b"\x19\xff\xff");
    alike(&u32::MAX, b"\x1a\xff\xff\xff\xff");
    alike(&u64::MAX, b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff");
    alike(&-1i8, b"\x20");
    alike(&i8::MIN, b"\x38\x7f");
    alike(&i32::MIN, b"\x3a\x7f\xff\xff\xff");
    alike(&i64::MIN, b"\x3b\x7f\xff\xff\xff\xff\xff\xff\xff");
}

#[test]
fn wide_integers_are_plain_until_they_need_a_bignum() {
    alike(
        &u128::from(u64::MAX),
        b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff",
    );
    alike(
        &(u128::from(u64::MAX) + 1),
        b"\xc2\x49\x01\x00\x00\x00\x00\x00\x00\x00\x00",
    );
    alike(&u128::MAX, &[[0xc2, 0x50].as_slice(), &[0xff; 16]].concat());
    alike(
        &(-i128::from(u64::MAX) - 1),
        b"\x3b\xff\xff\xff\xff\xff\xff\xff\xff",
    );
    alike(
        &(-i128::from(u64::MAX) - 2),
        b"\xc3\x49\x01\x00\x00\x00\x00\x00\x00\x00\x00",
    );
    alike(
        &i128::MIN,
        &[[0xc3, 0x50, 0x7f].as_slice(), &[0xff; 15]].concat(),
    );
    alike(
        &i128::MAX,
        &[[0xc2, 0x50, 0x7f].as_slice(), &[0xff; 15]].concat(),
    );
}

#[test]
fn floats_take_their_shortest_exact_width() {
    alike(&1.5f64, b"\xf9\x3e\x00");
    alike(&-0.0f64, b"\xf9\x80\x00");
    alike(&f64::INFINITY, b"\xf9\x7c\x00");
    alike(&0.1f32, b"\xfa\x3d\xcc\xcc\xcd");
    alike(&1.5f32, b"\xf9\x3e\x00");
    alike(&0.1f64, b"\xfb\x3f\xb9\x99\x99\x99\x99\x99\x9a");
    alike(&f64::from(0.1f32), b"\xfa\x3d\xcc\xcc\xcd");
}

#[test]
fn strings_bools_and_chars() {
    alike(&"", b"\x60");
    alike(&"abc", b"\x63abc");
    alike(&true, b"\xf5");
    alike(&false, b"\xf4");
    alike(&'a', b"\x61a");
    alike(&'\u{1f600}', b"\x64\xf0\x9f\x98\x80");
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Frame<'a> {
    #[serde(with = "serde_bytes")]
    data: &'a [u8],
    seq: u8,
}

#[derive(cbor_static::Cbor, Debug, PartialEq)]
struct NativeFrame<'a> {
    data: &'a [u8],
    seq: u8,
}

#[test]
fn byte_strings_through_serde_bytes_are_the_native_byte_strings() {
    let bytes = b"\xa2\x64data\x42\x01\x02\x63seq\x03";
    serde_only(
        &Frame {
            data: &[1, 2],
            seq: 3,
        },
        bytes,
    );
    let mut native = [0u8; 16];
    let len = cbor_static::to_slice(
        &NativeFrame {
            data: &[1, 2],
            seq: 3,
        },
        &mut native,
    )
    .expect("native encode");
    assert_eq!(&native[..len], bytes);
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Wide {
    Pair(u8, bool),
}

#[test]
fn a_tuple_variant_is_its_name_over_an_array() {
    serde_only(&Wide::Pair(1, true), b"\xa1\x64Pair\x82\x01\xf5");
}

#[test]
fn a_custom_error_is_placed_where_encoding_stopped() {
    struct Refusing;

    impl Serialize for Refusing {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("no"))
        }
    }

    assert_eq!(
        cbor_static::serde::to_slice(&(1u8, Refusing), &mut [0u8; 8])
            .map_err(|e| (e.kind, e.offset)),
        Err((cbor_static::ErrorKind::Custom, 2))
    );
}

#[test]
fn a_full_buffer_stops_where_the_native_encoder_does() {
    fn alike<T: Serialize + Encode>(value: &T) {
        let len = cbor_static::to_vec(value).expect("native encode").len();
        for room in 0..len {
            assert_eq!(
                cbor_static::serde::to_slice(value, &mut vec![0u8; room])
                    .map_err(|e| (e.kind, e.offset)),
                cbor_static::to_slice(value, &mut vec![0u8; room]).map_err(|e| (e.kind, e.offset)),
                "{room} of {len} bytes"
            );
        }
    }

    alike(&(1u8, "abc"));
    alike(&70_000u32);
    alike(&0.1f64);
    alike(&Meters(70_000));
    alike(&(1u8, "x".repeat(300).as_str()));
}

#[test]
fn neither_direction_is_human_readable() {
    use serde::{Deserializer as _, Serializer as _};

    let serializer = &mut cbor_static::serde::Serializer::new(&mut []);
    let deserializer = &mut cbor_static::serde::Deserializer::new(&[]);

    assert!(!serializer.is_human_readable());
    assert!(!deserializer.is_human_readable());
}
