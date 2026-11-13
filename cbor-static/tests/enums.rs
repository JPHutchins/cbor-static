#![forbid(unsafe_code)]

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{decode, decode_strict, golden, golden_contiguous};

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

/// Gaps, a negative discriminant, and implicit discriminants after explicit ones.
#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(index)]
enum Access {
    Read = 0,
    Write = 2,
    Erase,
    Reset = -3,
    Halt,
}

/// Explicit discriminants on variants with payloads, which Rust allows only under a primitive
/// `repr`.
#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
#[cbor(index)]
enum Frame {
    Ping = 1,
    Data(u32) = 5,
    Ack,
    Close { code: u16 } = 200,
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
fn indexed_variants_are_their_discriminant_then_their_payload() {
    golden(&Command::Ping, b"\x00");
    golden(&Command::Set(5), b"\x82\x01\x05");
    golden(
        &Command::Move { x: 1, y: -1 },
        b"\x82\x02\xa2\x61x\x01\x61y\x20",
    );
    golden(&Op::Nop, b"\x00");
    golden(&Op::Load(7), b"\x82\x01\x07");
    assert_eq!(decode::<Command>(b"\x9f\x01\x05\xff"), Ok(Command::Set(5)));
    assert_eq!(decode::<Command>(b"\x18\x00"), Ok(Command::Ping));
    assert_eq!(
        decode_strict::<Command>(b"\x18\x00"),
        Err(ErrorKind::NotDeterministic)
    );
    let errors: [(&[u8], ErrorKind); 9] = [
        (b"\x03", ErrorKind::UnknownVariant),
        (b"\x20", ErrorKind::UnknownVariant),
        (b"\x82\x19\x01\x00\x00", ErrorKind::UnknownVariant),
        (b"\x81\x00", ErrorKind::TypeMismatch),
        (b"\x01", ErrorKind::TypeMismatch),
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
fn indexed_variants_carry_their_rust_discriminants() {
    for (access, bytes) in [
        (Access::Read, b"\x00"),
        (Access::Write, b"\x02"),
        (Access::Erase, b"\x03"),
        (Access::Reset, b"\x22"),
        (Access::Halt, b"\x21"),
    ] {
        golden(&access, bytes);
    }
    assert_eq!(decode::<Access>(b"\x38\x02"), Ok(Access::Reset));
    assert_eq!(decode::<Access>(b"\x01"), Err(ErrorKind::UnknownVariant));
    golden(&Frame::Ping, b"\x01");
    golden(&Frame::Data(7), b"\x82\x05\x07");
    golden(&Frame::Ack, b"\x06");
    golden(&Frame::Close { code: 1 }, b"\x82\x18\xc8\xa1\x64code\x01");
    assert_eq!(
        decode::<Frame>(b"\x82\x01\x07"),
        Err(ErrorKind::TypeMismatch)
    );
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

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Size {
    w: u32,
    h: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "type")]
enum Event {
    Start,
    Move { x: i32, y: i32 },
    Resize(Size),
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "t", content = "c")]
enum Message {
    Ping,
    Data(u32),
    Point { x: i32, y: i32 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "t")]
enum Kind {
    A,
    B,
}

#[test]
fn internally_tagged_variants_merge_the_tag_into_their_map() {
    golden_contiguous(&Event::Start, b"\xa1\x64type\x65Start");
    golden_contiguous(&Kind::B, b"\xa1\x61t\x61B");
    golden_contiguous(
        &Event::Move { x: 1, y: 2 },
        b"\xa3\x64type\x64Move\x61x\x01\x61y\x02",
    );
    golden_contiguous(
        &Event::Resize(Size { w: 3, h: 4 }),
        b"\xa3\x64type\x66Resize\x61w\x03\x61h\x04",
    );
    assert_eq!(
        decode::<Event>(b"\xa3\x61x\x01\x61y\x02\x64type\x64Move"),
        Ok(Event::Move { x: 1, y: 2 }),
        "the tag may follow the payload's fields"
    );
    let errors: [(&[u8], ErrorKind); 4] = [
        (b"\xa1\x61x\x01", ErrorKind::MissingField),
        (b"\xa1\x64type\x63Foo", ErrorKind::UnknownVariant),
        (b"\xa1\x64type\x01", ErrorKind::TypeMismatch),
        (b"\x82\x01\x02", ErrorKind::TypeMismatch),
    ];
    for (bytes, error) in errors {
        assert_eq!(decode::<Event>(bytes), Err(error), "{bytes:x?}");
    }
}

#[test]
fn adjacently_tagged_variants_put_their_payload_under_the_content_key() {
    golden_contiguous(&Message::Ping, b"\xa1\x61t\x64Ping");
    golden_contiguous(&Message::Data(7), b"\xa2\x61t\x64Data\x61c\x07");
    golden_contiguous(
        &Message::Point { x: 1, y: 2 },
        b"\xa2\x61t\x65Point\x61c\xa2\x61x\x01\x61y\x02",
    );
    assert_eq!(
        decode::<Message>(b"\xa2\x61c\x07\x61t\x64Data"),
        Ok(Message::Data(7)),
        "the content may come first"
    );
    let errors: [(&[u8], ErrorKind); 3] = [
        (b"\xa1\x61t\x64Data", ErrorKind::MissingField),
        (b"\xa2\x61t\x64Ping\x61c\x00", ErrorKind::TypeMismatch),
        (b"\xa1\x61c\x07", ErrorKind::MissingField),
    ];
    for (bytes, error) in errors {
        assert_eq!(decode::<Message>(bytes), Err(error), "{bytes:x?}");
    }
}
