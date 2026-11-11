#![forbid(unsafe_code)]

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{decode, golden};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Coord {
    x: i32,
    y: i32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
enum Value {
    Missing,
    Count(u32),
    Flag(bool),
    Point(Coord),
    Pair([i32; 2]),
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
enum Shape {
    Dot,
    Sized(u32),
    At { x: i32, y: i32 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
#[serde(untagged)]
enum Small {
    Nothing,
    Num(u32),
    Flag(bool),
}

#[test]
fn untagged_variants_are_told_apart_by_their_initial_byte() {
    golden(&Value::Missing, b"\xf6");
    golden(&Value::Count(7), b"\x07");
    golden(&Value::Flag(true), b"\xf5");
    golden(
        &Value::Point(Coord { x: 1, y: 2 }),
        b"\xa2\x61x\x01\x61y\x02",
    );
    golden(&Value::Pair([-1, 1]), b"\x82\x20\x01");
    golden(&Shape::Dot, b"\xf6");
    golden(&Shape::Sized(300), b"\x19\x01\x2c");
    golden(&Shape::At { x: -1, y: 0 }, b"\xa2\x61x\x20\x61y\x00");
    golden(&Small::Nothing, b"\xf6");
    golden(&Small::Num(24), b"\x18\x18");
    golden(&Small::Flag(false), b"\xf4");
}

#[test]
fn an_item_no_untagged_variant_starts_with_is_rejected() {
    assert_eq!(decode::<Value>(b"\x61a"), Err(ErrorKind::TypeMismatch));
    assert_eq!(decode::<Shape>(b"\x20"), Err(ErrorKind::TypeMismatch));
    assert_eq!(decode::<Small>(b"\xa0"), Err(ErrorKind::TypeMismatch));
}
