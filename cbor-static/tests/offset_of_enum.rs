//! R-129: `#[cbor(offset_of_enum)]` takes a primitive-`repr` enum's payload offsets from the enum
//! itself, through nightly's `offset_of_enum`; every golden must match the shadow-struct path's.
#![cfg(feature = "nightly")]
#![feature(offset_of_enum)]
#![forbid(unsafe_code)]

mod common;

use cbor_static::Cbor;
use common::golden;

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
#[cbor(offset_of_enum)]
enum TagByEnum {
    Unit,
    One(u32),
    Two { a: u16, b: u64 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum TagByShadows {
    Unit,
    One(u32),
    Two { a: u16, b: u64 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
#[cbor(index, offset_of_enum)]
enum UnionByEnum {
    Unit,
    One(u32),
    Two { a: u16, b: u64 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
#[cbor(index)]
enum UnionByShadows {
    Unit,
    One(u32),
    Two { a: u16, b: u64 },
}

#[test]
fn offsets_from_the_enum_give_the_shadows_goldens() {
    for (by_enum, by_shadows, bytes) in [
        (TagByEnum::Unit, TagByShadows::Unit, b"\x64Unit".as_slice()),
        (TagByEnum::One(7), TagByShadows::One(7), b"\xa1\x63One\x07"),
        (
            TagByEnum::Two { a: 1, b: 2 },
            TagByShadows::Two { a: 1, b: 2 },
            b"\xa1\x63Two\xa2\x61a\x01\x61b\x02",
        ),
    ] {
        golden(&by_enum, bytes);
        golden(&by_shadows, bytes);
    }
    for (by_enum, by_shadows, bytes) in [
        (UnionByEnum::Unit, UnionByShadows::Unit, b"\x00".as_slice()),
        (UnionByEnum::One(7), UnionByShadows::One(7), b"\x82\x01\x07"),
        (
            UnionByEnum::Two { a: 1, b: 2 },
            UnionByShadows::Two { a: 1, b: 2 },
            b"\x82\x02\xa2\x61a\x01\x61b\x02",
        ),
    ] {
        golden(&by_enum, bytes);
        golden(&by_shadows, bytes);
    }
}
