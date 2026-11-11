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

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(index)]
enum Command {
    Ping,
    Set(u32),
    Move { x: i32, y: i32 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
#[cbor(index)]
enum Op {
    Nop,
    Load(u64),
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(array)]
struct Sign1 {
    protected: u32,
    payload: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
enum Cose {
    #[cbor(cbor_tag = 18)]
    Sign1(Sign1),
    #[cbor(cbor_tag = 17)]
    Mac0 { tag: u32 },
    #[cbor(cbor_tag = 1000)]
    Big(u64),
}

#[test]
fn indexed_variants_are_their_index_then_their_payload() {
    golden(&Command::Ping, b"\x81\x00");
    golden(&Command::Set(5), b"\x82\x01\x05");
    golden(
        &Command::Move { x: 1, y: -1 },
        b"\x82\x02\xa2\x61x\x01\x61y\x20",
    );
    golden(&Op::Nop, b"\x81\x00");
    golden(&Op::Load(7), b"\x82\x01\x07");
    assert_eq!(decode::<Command>(b"\x9f\x01\x05\xff"), Ok(Command::Set(5)));
    let errors: [(&[u8], ErrorKind); 6] = [
        (b"\x81\x03", ErrorKind::UnknownVariant),
        (b"\x82\x19\x01\x00\x00", ErrorKind::UnknownVariant),
        (b"\x81\x01", ErrorKind::TypeMismatch),
        (b"\x82\x00\x05", ErrorKind::TypeMismatch),
        (b"\x83\x01\x05\x06", ErrorKind::TypeMismatch),
        (b"\xa0", ErrorKind::TypeMismatch),
    ];
    for (bytes, error) in errors {
        assert_eq!(decode::<Command>(bytes), Err(error), "{bytes:x?}");
    }
}

#[test]
fn cbor_tagged_variants_are_their_tag_then_their_payload() {
    golden(
        &Cose::Sign1(Sign1 {
            protected: 1,
            payload: 2,
        }),
        b"\xd2\x82\x01\x02",
    );
    golden(&Cose::Mac0 { tag: 9 }, b"\xd1\xa1\x63tag\x09");
    golden(&Cose::Big(1), b"\xd9\x03\xe8\x01");
    assert_eq!(
        decode::<Cose>(b"\xd8\x12\x82\x01\x02"),
        Ok(Cose::Sign1(Sign1 {
            protected: 1,
            payload: 2
        }))
    );
    assert_eq!(decode::<Cose>(b"\xd3\x00"), Err(ErrorKind::UnexpectedTag));
    assert_eq!(
        decode::<Cose>(b"\x82\x01\x02"),
        Err(ErrorKind::TypeMismatch)
    );
}
