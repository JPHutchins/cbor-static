#![cfg(feature = "serde")]
//! Decoding through the serde layer refuses what the native decoder refuses, with the same kind at
//! the same offset (D-12), and nests only as deep as its limit (R-303).

use core::fmt::Debug;

use cbor_static::serde::Deserializer;
use cbor_static::{Decode, ErrorKind};
use serde::Deserialize;

/// `bytes` is refused as `T` by both layers alike, with `kind` at `offset`.
fn refused<'a, T>(bytes: &'a [u8], kind: ErrorKind, offset: u32)
where
    T: Deserialize<'a> + Decode<'a> + Debug,
{
    assert_eq!(
        cbor_static::from_slice::<T>(bytes)
            .err()
            .map(|e| (e.kind, e.offset)),
        Some((kind, offset)),
        "native {bytes:02x?}"
    );
    serde_refused::<T>(bytes, kind, offset);
}

fn serde_refused<'a, T: Deserialize<'a> + Debug>(bytes: &'a [u8], kind: ErrorKind, offset: u32) {
    assert_eq!(
        cbor_static::serde::from_slice::<T>(bytes)
            .err()
            .map(|e| (e.kind, e.field, e.offset)),
        Some((kind, 0, offset)),
        "serde {bytes:02x?}"
    );
}

/// `bytes` decodes as `T` to `value` through both layers.
fn accepted<'a, T>(bytes: &'a [u8], value: &T)
where
    T: Deserialize<'a> + Decode<'a> + PartialEq + Debug,
{
    assert_eq!(
        cbor_static::from_slice::<T>(bytes).as_ref(),
        Ok(value),
        "native"
    );
    assert_eq!(
        cbor_static::serde::from_slice::<T>(bytes).as_ref(),
        Ok(value),
        "serde"
    );
}

#[test]
fn integers_narrow_only_in_range() {
    accepted(b"\x18\xff", &255u8);
    refused::<u8>(b"\x19\x01\x00", ErrorKind::OutOfRange, 3);
    refused::<i8>(b"\x38\x80", ErrorKind::OutOfRange, 2);
    accepted(b"\x38\x7f", &-128i8);
    refused::<u64>(b"\x20", ErrorKind::OutOfRange, 1);
    refused::<i64>(
        b"\x3b\x80\x00\x00\x00\x00\x00\x00\x00",
        ErrorKind::OutOfRange,
        9,
    );
    accepted(b"\x3b\x7f\xff\xff\xff\xff\xff\xff\xff", &i64::MIN);
}

#[test]
fn integers_and_floats_refuse_each_other() {
    refused::<u32>(b"\xf9\x3c\x00", ErrorKind::TypeMismatch, 3);
    refused::<i16>(
        b"\xfb\x3f\xf0\x00\x00\x00\x00\x00\x00",
        ErrorKind::TypeMismatch,
        9,
    );
    refused::<f64>(b"\x01", ErrorKind::TypeMismatch, 1);
    refused::<f32>(b"\x20", ErrorKind::TypeMismatch, 1);
}

#[test]
fn floats_narrow_only_when_exact() {
    accepted(b"\xfb\x3f\xf8\x00\x00\x00\x00\x00\x00", &1.5f32);
    accepted(b"\xf9\x3e\x00", &1.5f64);
    refused::<f32>(
        b"\xfb\x3f\xb9\x99\x99\x99\x99\x99\x9a",
        ErrorKind::OutOfRange,
        9,
    );
}

#[test]
fn wide_integers_take_bignums_in_range() {
    accepted(b"\xc2\x42\x01\x00", &256u128);
    accepted(b"\xc2\x43\x00\x01\x00", &256u128);
    accepted(b"\xc3\x42\x01\x00", &-257i128);
    accepted(&[[0xc2, 0x50].as_slice(), &[0xff; 16]].concat(), &u128::MAX);
    refused::<u128>(b"\x20", ErrorKind::OutOfRange, 1);
    refused::<u128>(b"\xc3\x41\x01", ErrorKind::OutOfRange, 1);
    refused::<i128>(
        &[[0xc2, 0x50].as_slice(), &[0xff; 16]].concat(),
        ErrorKind::OutOfRange,
        18,
    );
    refused::<u128>(
        &[[0xc2, 0x51, 0x01].as_slice(), &[0x00; 16]].concat(),
        ErrorKind::OutOfRange,
        19,
    );
    refused::<u64>(b"\xc2\x41\x01", ErrorKind::TypeMismatch, 1);
    refused::<u128>(b"\xc1\x01", ErrorKind::TypeMismatch, 1);
}

#[test]
fn tags_and_simple_values_mismatch() {
    refused::<u32>(b"\xc1\x01", ErrorKind::TypeMismatch, 1);
    refused::<&str>(b"\xd8\x20\x61a", ErrorKind::TypeMismatch, 2);
    refused::<bool>(b"\xf7", ErrorKind::TypeMismatch, 1);
    refused::<Option<u32>>(b"\xf7", ErrorKind::TypeMismatch, 1);
    refused::<()>(b"\x00", ErrorKind::TypeMismatch, 1);
}

#[test]
fn a_refused_item_fails_where_native_reports_it() {
    refused::<u32>(b"\x65hello", ErrorKind::TypeMismatch, 1);
    refused::<&[u8]>(b"\x62ab", ErrorKind::TypeMismatch, 3);
    refused::<&str>(b"\x42ab", ErrorKind::TypeMismatch, 3);
    refused::<&str>(b"\x01", ErrorKind::TypeMismatch, 1);
}

/// One test per input: both layers refuse it with `$kind` at `$offset`.
macro_rules! refused_alike {
    ($($name:ident: $ty:ty, $bytes:expr, $kind:ident, $offset:literal;)*) => {
        $(
            #[test]
            fn $name() {
                refused::<$ty>($bytes, ErrorKind::$kind, $offset);
            }
        )*
    };
}

refused_alike! {
    a_byte_string_cut_short_into_a_u32: u32, b"\x41", TypeMismatch, 1;
    a_text_cut_short_into_a_u32: u32, b"\x61", TypeMismatch, 1;
    a_byte_string_cut_short_into_an_i128: i128, b"\x41", TypeMismatch, 1;
    a_text_cut_short_into_a_u128: u128, b"\x61", TypeMismatch, 1;
    a_byte_string_cut_short_into_an_f64: f64, b"\x41", TypeMismatch, 1;
    a_text_cut_short_into_a_bool: bool, b"\x61", TypeMismatch, 1;
    a_byte_string_cut_short_into_an_option: Option<u8>, b"\x41", TypeMismatch, 1;
    a_text_cut_short_into_unit: (), b"\x61", TypeMismatch, 1;
    a_byte_string_cut_short_into_an_array: [u8; 1], b"\x41", TypeMismatch, 1;
    a_text_cut_short_into_a_map: Point, b"\x61", TypeMismatch, 1;
    a_fields_byte_string_cut_short: Point, b"\xa1\x61x\x41", TypeMismatch, 4;
    a_fields_text_cut_short: Point, b"\xa1\x61x\x61", TypeMismatch, 4;
    invalid_text_into_a_u32: u32, b"\x62\xc3\x28", TypeMismatch, 1;
    invalid_text_into_a_bool: bool, b"\x61\xff", TypeMismatch, 1;
    invalid_text_into_an_array: [u8; 1], b"\x63a\xffb", TypeMismatch, 1;
    invalid_text_into_a_map: Point, b"\x62\xc3\x28", TypeMismatch, 1;
    a_fields_invalid_text: Point, b"\xa1\x61x\x62\xc3\x28", TypeMismatch, 4;
    invalid_text_into_bytes_past_its_payload: &[u8], b"\x62\xc3\x28", TypeMismatch, 3;
    bytes_into_a_char_at_their_head: char, b"\x41\xff", TypeMismatch, 1;
    bytes_cut_short_into_a_char_at_their_head: char, b"\x42a", TypeMismatch, 1;
    invalid_text_as_bignum_digits_at_its_head: u128, b"\xc2\x62\xc3\x28", TypeMismatch, 2;
    text_cut_short_as_bignum_digits_at_its_head: u128, b"\xc2\x63ab", TypeMismatch, 2;
    an_invalid_indefinite_text_into_a_u32: u32, b"\x7f\x62\xc3\x28\xff", TypeMismatch, 1;
    an_indefinite_text_cut_short_into_a_u32: u32, b"\x7f\x61", TypeMismatch, 1;
    an_indefinite_byte_string_cut_short_into_a_u32: u32, b"\x5f\x41", TypeMismatch, 1;
    bytes_into_text_past_their_payload: &str, b"\x42\xc3\x28", TypeMismatch, 3;
    invalid_text_into_a_char: char, b"\x62\xc3\x28", InvalidUtf8, 3;
    bytes_cut_short_as_bignum_digits: u128, b"\xc2\x43ab", UnexpectedEnd, 4;
    a_map_counted_past_2_63_into_a_u32: u32, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_a_bool: bool, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_an_f64: f64, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_an_option: Option<u32>, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_an_i128: i128, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_a_u128: u128, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_unit: (), b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_a_char: char, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_text: &str, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_bytes: &[u8], b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_into_an_array: [u8; 1], b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 9;
    a_map_counted_past_2_63_as_bignum_digits: u128, b"\xc2\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", TypeMismatch, 10;
}

#[test]
fn malformed_input_fails_where_native_reports_it() {
    refused::<&str>(b"\x63ab", ErrorKind::UnexpectedEnd, 1);
    refused::<&[u8]>(b"\x45ab", ErrorKind::UnexpectedEnd, 1);
    refused::<u16>(b"\x19\x01", ErrorKind::UnexpectedEnd, 2);
    refused::<Option<u8>>(b"\x18", ErrorKind::UnexpectedEnd, 1);
    refused::<&str>(b"\x62\xc3\x28", ErrorKind::InvalidUtf8, 3);
    refused::<[u8; 1]>(b"\x81\xff", ErrorKind::NotWellFormed, 2);
    refused::<bool>(b"\xf8\x14", ErrorKind::NotWellFormed, 2);
    refused::<Point>(b"\xa1\x62x", ErrorKind::UnexpectedEnd, 3);
    refused::<Point>(b"\xa1\x62\xc3\x28", ErrorKind::InvalidUtf8, 4);
    refused::<Point>(b"\xa1\x7f\x61x\xff\x01", ErrorKind::Unsupported, 2);
    refused::<char>(b"\x62a", ErrorKind::UnexpectedEnd, 2);
    refused::<u128>(b"\xc2\x42\x01", ErrorKind::UnexpectedEnd, 3);
    refused::<Shape>(b"\x66Sq", ErrorKind::UnexpectedEnd, 3);
    refused::<Shape>(b"\xa1\x66Sq", ErrorKind::UnexpectedEnd, 4);
    refused::<Point>(
        &[
            b"\xa2\x61y".as_slice(),
            &[0x9f; 65],
            &[0xff; 65],
            b"\x61x\x01",
        ]
        .concat(),
        ErrorKind::LimitExceeded,
        68,
    );
    refused::<[[u8; 0]; 1]>(b"\x9f\x9f\xff\x01", ErrorKind::TypeMismatch, 3);
    refused::<char>(b"\x62ab", ErrorKind::TypeMismatch, 3);
    accepted(b"\x62\xc3\xa9", &'é');
}

#[test]
fn a_borrowed_string_refuses_chunks() {
    let native = |bytes| cbor_static::from_slice::<&str>(bytes).map_err(|e| (e.kind, e.offset));
    assert_eq!(
        native(b"\x7f\x61a\xff"),
        Err((ErrorKind::IndefiniteString, 1))
    );
    if cfg!(feature = "alloc") {
        serde_refused::<&str>(b"\x7f\x61a\xff", ErrorKind::TypeMismatch, 4);
        serde_refused::<&[u8]>(b"\x5f\x41\x01\xff", ErrorKind::TypeMismatch, 4);
    } else {
        serde_refused::<&str>(b"\x7f\x61a\xff", ErrorKind::IndefiniteString, 1);
        serde_refused::<&[u8]>(b"\x5f\x41\x01\xff", ErrorKind::IndefiniteString, 1);
    }
}

#[test]
fn indefinite_arrays_and_maps_decode() {
    accepted(b"\x9f\x01\x02\xff", &[1u8, 2]);
    accepted(b"\x9f\xff", &[0u8; 0]);
    accepted(b"\xbf\x61x\x01\xff", &Point { x: 1 });
    assert_eq!(
        cbor_static::from_slice::<[u8; 2]>(b"\x9f\x01\xff").map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::TypeMismatch, 3)),
        "native: a break where an element is owed"
    );
    serde_refused::<[u8; 2]>(b"\x9f\x01\xff", ErrorKind::MissingField, 3);
    refused::<[u8; 1]>(b"\x9f\x01\x02\xff", ErrorKind::TypeMismatch, 2);
}

#[test]
fn a_short_or_long_array_is_refused_as_natively() {
    refused::<(u8, u8)>(b"\x81\x01", ErrorKind::MissingField, 2);
    refused::<(u8, u8)>(b"\x83\x01\x02\x03", ErrorKind::TypeMismatch, 3);
}

#[derive(Deserialize, cbor_static::Decode, Debug, PartialEq)]
struct Point {
    x: u32,
}

#[derive(Deserialize, cbor_static::Decode, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Strict {
    x: u32,
}

#[test]
fn struct_keys_follow_native_rules() {
    accepted(b"\xa2\x61y\x82\x01\xa1\x00\x00\x61x\x07", &Point { x: 7 });
    accepted(b"\xa2\x00\x01\x61x\x07", &Point { x: 7 });
    accepted(b"\xa2\x82\x00\x00\xf5\x61x\x07", &Point { x: 7 });
    refused::<Point>(b"\xa0", ErrorKind::MissingField, 1);
    refused::<Point>(b"\xa2\x61x\x01\x61x\x02", ErrorKind::DuplicateKey, 6);
    refused::<Strict>(b"\xa2\x61y\x00\x61x\x07", ErrorKind::UnknownField, 3);
    refused::<Strict>(b"\xa2\x00\x00\x61x\x07", ErrorKind::UnknownField, 2);
    refused::<Point>(b"\x81\x07", ErrorKind::TypeMismatch, 1);
}

#[test]
fn unknown_values_are_skipped_whole_and_checked() {
    accepted(
        b"\xa2\x61y\xbf\x61a\x9f\x01\xff\x61b\x5f\x41\x01\xff\xff\x61x\x07",
        &Point { x: 7 },
    );
    refused::<Point>(
        b"\xa2\x61y\x9f\x82\x01\xff\x61x\x07",
        ErrorKind::NotWellFormed,
        7,
    );
    refused::<Point>(b"\xa2\x61y\x62\xc3\x28\x61x\x07", ErrorKind::InvalidUtf8, 6);
}

#[derive(Deserialize, cbor_static::Decode, Debug, PartialEq)]
enum Shape {
    Empty,
    Square(u32),
    Circle { r: u32 },
}

#[test]
fn variants_take_their_external_form_only() {
    accepted(b"\x65Empty", &Shape::Empty);
    accepted(b"\xa1\x66Square\x02", &Shape::Square(2));
    accepted(b"\xbf\x66Square\x02\xff", &Shape::Square(2));
    accepted(b"\xa1\x66Circle\xa1\x61r\x01", &Shape::Circle { r: 1 });
    refused::<Shape>(b"\xa1\x65Empty\xf6", ErrorKind::TypeMismatch, 7);
    refused::<Shape>(b"\x66Square", ErrorKind::TypeMismatch, 7);
    refused::<Shape>(b"\x64Cone", ErrorKind::UnknownVariant, 5);
    refused::<Shape>(
        b"\xbf\x66Square\x02\x66Square\x03\xff",
        ErrorKind::TypeMismatch,
        10,
    );
    refused::<Shape>(
        b"\xa2\x66Square\x02\x65Empty\xf6",
        ErrorKind::TypeMismatch,
        1,
    );
    refused::<Shape>(b"\xa1\x01\x02", ErrorKind::TypeMismatch, 2);
    refused::<Shape>(b"\x7f\x65Empty\xff", ErrorKind::TypeMismatch, 1);
    refused::<Shape>(b"\x01", ErrorKind::TypeMismatch, 1);
}

#[test]
fn trailing_and_partial_input() {
    refused::<u8>(b"\x01\x02", ErrorKind::TrailingInput, 1);
    assert_eq!(
        cbor_static::serde::from_slice_partial::<u8>(b"\x01\x02\x03"),
        Ok((1, b"\x02\x03".as_slice()))
    );
    refused::<u8>(b"", ErrorKind::UnexpectedEnd, 0);
    refused::<(u8, u8)>(b"\x82\x01", ErrorKind::UnexpectedEnd, 2);
}

#[derive(Deserialize, Debug, PartialEq)]
struct Nest(Vec<Nest>);

/// `depth` arrays, each holding the next, the innermost empty.
fn nested(depth: usize) -> Vec<u8> {
    [vec![0x81; depth - 1], vec![0x80]].concat()
}

#[test]
fn nesting_stops_cleanly_at_the_default_limit() {
    assert!(cbor_static::serde::from_slice::<Nest>(&nested(32)).is_ok());
    serde_refused::<Nest>(&nested(33), ErrorKind::LimitExceeded, 32);
    serde_refused::<Nest>(&nested(100_000), ErrorKind::LimitExceeded, 32);
}

#[test]
fn leaving_a_container_gives_its_level_back() {
    let siblings = [b"\x98\x28".as_slice(), &[0x80; 40]].concat();
    assert_eq!(
        cbor_static::serde::from_slice::<Vec<[u8; 0]>>(&siblings).map(|items| items.len()),
        Ok(40)
    );
    assert_eq!(
        Deserializer::with_depth_limit(&siblings, 2)
            .decode::<Vec<[u8; 0]>>()
            .map(|items| items.len()),
        Ok(40)
    );
}

#[test]
fn nesting_stops_cleanly_at_a_configured_limit() {
    let at = |limit: u8, depth: usize| {
        Deserializer::with_depth_limit(&nested(depth), limit)
            .decode::<Nest>()
            .err()
            .map(|e| (e.kind, e.offset))
    };
    assert_eq!(at(4, 4), None);
    assert_eq!(at(4, 5), Some((ErrorKind::LimitExceeded, 4)));
    assert_eq!(at(255, 255), None);
    assert_eq!(at(255, 256), Some((ErrorKind::LimitExceeded, 255)));
    assert_eq!(at(0, 1), Some((ErrorKind::LimitExceeded, 0)));
}

#[test]
fn maps_and_variants_count_toward_depth() {
    let deep_map = [b"\xa1\x61a".repeat(32), vec![0xa0]].concat();
    let map_of_maps = |depth: u8| {
        Deserializer::with_depth_limit(&deep_map, depth)
            .decode::<serde::de::IgnoredAny>()
            .map(|_| ())
            .map_err(|e| e.kind)
    };
    assert_eq!(map_of_maps(0), Ok(()));
    assert_eq!(
        Deserializer::with_depth_limit(b"\xa1\x66Square\x02", 0)
            .decode::<Shape>()
            .err()
            .map(|e| (e.kind, e.offset)),
        Some((ErrorKind::LimitExceeded, 0))
    );
    assert_eq!(
        Deserializer::with_depth_limit(b"\xa1\x61x\x07", 0)
            .decode::<Point>()
            .err()
            .map(|e| (e.kind, e.offset)),
        Some((ErrorKind::LimitExceeded, 0))
    );
}

/// A struct whose hand-written visitor reads its keys as strings, not as identifiers.
#[derive(Debug, PartialEq)]
struct Keyed(u32);

impl<'de> Deserialize<'de> for Keyed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Keyed;

            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                formatter.write_str("a map of one key")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Keyed, A::Error> {
                let mut value = 0;
                while let Some(key) = map.next_key::<&str>()? {
                    value += map.next_value::<u32>()? + u32::try_from(key.len()).unwrap_or(0);
                }
                Ok(Keyed(value))
            }
        }

        deserializer.deserialize_struct("Keyed", &["x"], Visitor)
    }
}

#[test]
fn a_skipped_key_reads_only_as_an_identifier_or_ignored() {
    assert_eq!(
        cbor_static::serde::from_slice::<Keyed>(b"\xa1\x61x\x07"),
        Ok(Keyed(8))
    );
    serde_refused::<Keyed>(b"\xa1\x00\x07", ErrorKind::TypeMismatch, 2);
    assert_eq!(
        cbor_static::serde::from_slice::<serde::de::IgnoredAny>(b"\xa1\x00\x07").map(|_| ()),
        Ok(())
    );
    assert_eq!(
        cbor_static::serde::from_slice::<Unkeyed>(b"\xa1\x00\x07"),
        Ok(Unkeyed(7))
    );
}

/// A key seed that never reads its key.
struct Unread;

impl<'de> serde::de::DeserializeSeed<'de> for Unread {
    type Value = ();

    fn deserialize<D: serde::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Ok(())
    }
}

/// A struct of one entry whose hand-written visitor reads its key with [`Unread`].
#[derive(Debug, PartialEq)]
struct Unkeyed(u32);

impl<'de> Deserialize<'de> for Unkeyed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Unkeyed;

            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                formatter.write_str("a map of one entry")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Unkeyed, A::Error> {
                map.next_key_seed(Unread)?;
                map.next_value().map(Unkeyed)
            }
        }

        deserializer.deserialize_struct("Unkeyed", &["x"], Visitor)
    }
}
