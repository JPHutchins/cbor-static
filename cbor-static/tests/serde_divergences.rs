#![cfg(all(feature = "serde", feature = "alloc"))]
//! Where the serde layer knowingly refuses an input otherwise than the native decoder: serde's
//! Deserializer is not told what the native leaf knows, so it decides later, and each pin breaks
//! if either layer changes.

use cbor_static::{Decode, ErrorKind};
use serde::Deserialize;

/// A borrowed text and byte string: the native leaves cannot assemble chunks into either.
#[derive(Deserialize, Decode, Debug)]
struct Borrowed<'a> {
    t: &'a str,
    #[serde(borrow, with = "serde_bytes")]
    b: &'a [u8],
}

#[derive(Deserialize, Decode, Debug)]
struct Point {
    x: u32,
}

/// One test per input: the native decoder refuses it as `$ty` with `$native`, the serde layer
/// with `$serde`.
macro_rules! refused_apart {
    ($($name:ident: $ty:ty, $bytes:expr, $native:ident @ $native_at:literal, $serde:ident @ $serde_at:literal;)*) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(
                    cbor_static::from_slice::<$ty>($bytes).map(drop).map_err(|e| (e.kind, e.offset)),
                    Err((ErrorKind::$native, $native_at)),
                    "native"
                );
                assert_eq!(
                    cbor_static::serde::from_slice::<$ty>($bytes).map(drop).map_err(|e| (e.kind, e.offset)),
                    Err((ErrorKind::$serde, $serde_at)),
                    "serde"
                );
            }
        )*
    };
}

refused_apart! {
    a_valid_indefinite_text_into_a_borrowed_str: Borrowed, b"\xa1\x61t\x7f\x61a\xff", IndefiniteString @ 4, TypeMismatch @ 7;
    an_empty_indefinite_text_into_a_borrowed_str: Borrowed, b"\xa1\x61t\x7f\xff", IndefiniteString @ 4, TypeMismatch @ 5;
    an_indefinite_text_cut_short_into_a_borrowed_str: Borrowed, b"\xa1\x61t\x7f", IndefiniteString @ 4, UnexpectedEnd @ 4;
    a_nested_indefinite_chunk_into_a_borrowed_str: Borrowed, b"\xa1\x61t\x7f\x7f\xff\xff", IndefiniteString @ 4, NotWellFormed @ 5;
    a_byte_chunk_into_a_borrowed_str: Borrowed, b"\xa1\x61t\x7f\x41a\xff", IndefiniteString @ 4, NotWellFormed @ 5;
    an_invalid_chunk_into_a_borrowed_str: Borrowed, b"\xa1\x61t\x7f\x61\xff\xff", IndefiniteString @ 4, InvalidUtf8 @ 6;
    a_valid_indefinite_byte_string_into_borrowed_bytes: Borrowed, b"\xa1\x61b\x5f\x41a\xff", IndefiniteString @ 4, TypeMismatch @ 7;
    an_empty_indefinite_byte_string_into_borrowed_bytes: Borrowed, b"\xa1\x61b\x5f\xff", IndefiniteString @ 4, TypeMismatch @ 5;
    an_indefinite_byte_string_cut_short_into_borrowed_bytes: Borrowed, b"\xa1\x61b\x5f", IndefiniteString @ 4, UnexpectedEnd @ 4;
    a_nested_indefinite_chunk_into_borrowed_bytes: Borrowed, b"\xa1\x61b\x5f\x5f\xff\xff", IndefiniteString @ 4, NotWellFormed @ 5;
    a_map_counted_past_2_63_into_a_struct: Point, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63", UnexpectedEnd @ 9, LimitExceeded @ 9;
    a_map_counted_past_2_63_with_an_invalid_key_into_a_struct: Point, b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63\x61\xff\x00", InvalidUtf8 @ 11, LimitExceeded @ 9;
    a_key_array_counted_2_64_minus_1_into_a_struct: Point, b"\xa1\x9b\xff\xff\xff\xff\xff\xff\xff\xff\xff", LimitExceeded @ 10, NotWellFormed @ 11;
    a_key_array_counted_2_64_minus_1_then_more_into_a_struct: Point, b"\xa1\x9b\xff\xff\xff\xff\xff\xff\xff\xff\x00", LimitExceeded @ 10, UnexpectedEnd @ 11;
    a_key_array_counted_2_64_minus_1_cut_short_into_a_struct: Point, b"\xa1\x9b\xff\xff\xff\xff\xff\xff\xff\xff", LimitExceeded @ 10, UnexpectedEnd @ 10;
    a_long_array_into_a_fixed_array: [u8; 2], b"\x83\x01\x02\x03", TypeMismatch @ 1, TypeMismatch @ 3;
    a_short_array_into_a_fixed_array: [u8; 2], b"\x81\x01", TypeMismatch @ 1, MissingField @ 2;
    a_long_array_cut_short_into_a_fixed_array: [u8; 2], b"\x83\x01", TypeMismatch @ 1, UnexpectedEnd @ 2;
    a_huge_array_into_a_fixed_array: [u8; 2], b"\x9b\x00\x00\x00\x01\x00\x00\x00\x00", TypeMismatch @ 9, UnexpectedEnd @ 9;
    a_short_indefinite_array_into_a_fixed_array: [u8; 2], b"\x9f\x01\xff", TypeMismatch @ 3, MissingField @ 3;
}

/// Why the serde layer cannot refuse an indefinite string at the head as the native borrowed
/// leaf does: `deserialize_str` is a hint, and any visitor that takes a transient `&str` accepts
/// the assembled text.
#[test]
fn an_assembled_indefinite_text_reaches_a_visitor_of_any_str() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct Owned<'a> {
        #[serde(borrow)]
        t: std::borrow::Cow<'a, str>,
    }

    assert_eq!(
        cbor_static::serde::from_slice::<Owned>(b"\xa1\x61t\x7f\x61a\x61a\xff"),
        Ok(Owned { t: "aa".into() })
    );
}
