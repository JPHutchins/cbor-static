#![forbid(unsafe_code)]

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{decode, golden};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(array)]
struct Point {
    x: i32,
    y: i32,
    label: Option<u32>,
    weight: Option<u32>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Body {
    a: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(array)]
struct Envelope {
    id: u32,
    body: Body,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Pair {
    range: (u32, u32),
    tag: (bool, Option<u32>, i32, u64),
}

const POINT: Point = Point {
    x: 1,
    y: -2,
    label: Some(7),
    weight: Some(9),
};

#[test]
fn array_structs_are_positional_and_truncate_trailing_nones() {
    golden(&POINT, b"\x84\x01\x21\x07\x09");
    golden(
        &Point {
            label: None,
            weight: None,
            ..POINT
        },
        b"\x82\x01\x21",
    );
    golden(
        &Point {
            label: None,
            ..POINT
        },
        b"\x84\x01\x21\xf6\x09",
    );
    golden(
        &Point {
            weight: None,
            ..POINT
        },
        b"\x83\x01\x21\x07",
    );
    golden(
        &Envelope {
            id: 1,
            body: Body { a: 2 },
        },
        b"\x82\x01\xa1\x61a\x02",
    );
}

#[test]
fn array_structs_decode_indefinite_arrays_and_reject_wrong_lengths() {
    assert_eq!(
        decode::<Point>(b"\x9f\x01\x21\x07\xff"),
        Ok(Point {
            weight: None,
            ..POINT
        })
    );
    assert_eq!(decode::<Point>(b"\x81\x01"), Err(ErrorKind::MissingField));
    assert_eq!(
        decode::<Point>(b"\x85\x01\x21\x07\x09\x00"),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(decode::<Point>(b"\xa0"), Err(ErrorKind::TypeMismatch));
}

#[test]
fn tuples_are_arrays_of_their_elements() {
    golden(&(7u32, true), b"\x82\x07\xf5");
    golden(&(-1i32,), b"\x81\x20");
    golden(
        &Pair {
            range: (1, 2),
            tag: (false, None, -1, 3),
        },
        b"\xa2\x65range\x82\x01\x02\x63tag\x84\xf4\xf6\x20\x03",
    );
    assert_eq!(
        decode::<(u32, bool)>(b"\x81\x07"),
        Err(ErrorKind::MissingField)
    );
}
