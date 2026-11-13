#![forbid(unsafe_code)]

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{decode, golden};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Protected {
    #[cbor(key = 1)]
    alg: i32,
    #[cbor(key = -1)]
    crv: u32,
    #[cbor(key = 4, default, skip_serializing_if = "Option::is_none")]
    kid: Option<u32>,
    #[cbor(key = 1000)]
    big: u64,
}

#[derive(Clone, Copy)]
enum CoseKey {
    Kty = 1,
    Alg = 3,
    Crv = -1,
    X = -2,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(keys = CoseKey)]
struct Ec2 {
    kty: u32,
    alg: i32,
    crv: u32,
    x: u64,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Mixed {
    #[cbor(key = 97)]
    x: u32,
    b: bool,
    #[cbor(key = -24)]
    n: i32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deny_unknown_fields)]
struct StrictText {
    a: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deny_unknown_fields)]
struct StrictInt {
    #[cbor(key = 1)]
    a: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deny_unknown_fields)]
struct StrictMixed {
    #[cbor(key = 1)]
    a: u32,
    b: u32,
}

const PROTECTED: Protected = Protected {
    alg: -7,
    crv: 1,
    kid: Some(5),
    big: 2,
};

#[test]
fn integer_keys_negative_ones_included() {
    golden(&PROTECTED, b"\xa4\x01\x26\x20\x01\x04\x05\x19\x03\xe8\x02");
    golden(
        &Protected {
            kid: None,
            ..PROTECTED
        },
        b"\xa3\x01\x26\x20\x01\x19\x03\xe8\x02",
    );
}

#[test]
fn integer_keys_accept_non_preferred_encodings_and_skip_unknown_entries() {
    assert_eq!(
        decode::<Protected>(
            b"\xa4\x18\x01\x38\x06\x39\x00\x00\x01\x04\x05\x1a\x00\x00\x03\xe8\x02"
        ),
        Ok(PROTECTED)
    );
    assert_eq!(
        decode::<Protected>(b"\xa5\x01\x26\x20\x01\x04\x05\x18\x63\x82\x01\x02\x19\x03\xe8\x02"),
        Ok(PROTECTED)
    );
}

#[test]
fn integer_key_errors() {
    assert_eq!(
        decode::<Protected>(b"\xa1\x01\x26"),
        Err(ErrorKind::MissingField)
    );
    assert_eq!(
        decode::<StrictInt>(b"\xa2\x01\x01\x01\x02"),
        Err(ErrorKind::DuplicateKey)
    );
}

#[test]
fn enum_keyed_structs_use_the_label_discriminants() {
    golden(
        &Ec2 {
            kty: 2,
            alg: -7,
            crv: 1,
            x: 0x1234,
        },
        b"\xa4\x01\x02\x03\x26\x20\x01\x21\x19\x12\x34",
    );
}

#[test]
fn mixed_keys_match_text_and_integers_apart() {
    let mixed = Mixed {
        x: 1,
        b: true,
        n: -100,
    };
    golden(&mixed, b"\xa3\x18\x61\x01\x61b\xf5\x37\x38\x63");
    assert_eq!(
        decode::<Mixed>(b"\xa4\x61a\x05\x18\x61\x01\x61b\xf5\x37\x38\x63"),
        Ok(mixed),
        "the text key \"a\" is not the integer key 97, whose head ends in the same byte"
    );
}

#[test]
fn deny_unknown_fields_rejects_any_entry_no_field_claims() {
    golden(&StrictText { a: 1 }, b"\xa1\x61a\x01");
    golden(&StrictInt { a: 1 }, b"\xa1\x01\x01");
    golden(&StrictMixed { a: 1, b: 2 }, b"\xa2\x01\x01\x61b\x02");
    for bytes in [
        b"\xa2\x61a\x01\x61z\x02".as_slice(),
        b"\xa2\x61a\x01\x03\x02",
    ] {
        assert_eq!(
            decode::<StrictText>(bytes),
            Err(ErrorKind::UnknownField),
            "{bytes:x?}"
        );
    }
    for bytes in [b"\xa2\x01\x01\x09\x00".as_slice(), b"\xa2\x01\x01\x61a\x00"] {
        assert_eq!(
            decode::<StrictInt>(bytes),
            Err(ErrorKind::UnknownField),
            "{bytes:x?}"
        );
    }
    for bytes in [
        b"\xa3\x01\x01\x61b\x02\x61c\x03".as_slice(),
        b"\xa3\x01\x01\x61b\x02\x02\x03",
    ] {
        assert_eq!(
            decode::<StrictMixed>(bytes),
            Err(ErrorKind::UnknownField),
            "{bytes:x?}"
        );
    }
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct Camel {
    key_ops: u32,
    #[serde(rename = "Alg")]
    alg_id: u32,
    #[cbor(key = 4)]
    kid: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "PascalCase")]
struct Pascal {
    key_ops: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
struct Snake {
    key_ops: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Op {
    ReadAll,
    WriteOne(u32),
    #[serde(rename = "Seek")]
    SeekTo(u32),
    Nested {
        inner_field: u32,
    },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Event {
    MoveTo { x: i32 },
}

#[test]
fn rename_all_renames_unrenamed_text_keys_and_variant_names_as_serde_does() {
    golden(
        &Camel {
            key_ops: 1,
            alg_id: 2,
            kid: 3,
        },
        b"\xa3\x66keyOps\x01\x63Alg\x02\x04\x03",
    );
    golden(&Pascal { key_ops: 1 }, b"\xa1\x66KeyOps\x01");
    golden(&Snake { key_ops: 1 }, b"\xa1\x67key_ops\x01");
    golden(&Op::ReadAll, b"\x68read_all");
    golden(&Op::WriteOne(5), b"\xa1\x69write_one\x05");
    golden(&Op::SeekTo(6), b"\xa1\x64Seek\x06");
    golden(
        &Op::Nested { inner_field: 1 },
        b"\xa1\x66nested\xa1\x6binner_field\x01",
    );
    assert_eq!(
        decode::<Event>(b"\xa2\x64type\x66moveTo\x61x\x01"),
        Ok(Event::MoveTo { x: 1 })
    );
    assert_eq!(decode::<Op>(b"\x67ReadAll"), Err(ErrorKind::UnknownVariant));
}

#[derive(Clone, Copy)]
enum KeyParam {
    Kty = 1,
    KeyOps = 4,
    BaseIv = 5,
}

/// R-122: a field's `snake_case` name maps to its `keys` variant in `UpperCamelCase`.
#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(keys = KeyParam)]
struct Params {
    kty: u32,
    key_ops: u32,
    base_iv: u32,
}

#[test]
fn a_snake_case_field_takes_its_upper_camel_case_variant_as_key() {
    golden(
        &Params {
            kty: 2,
            key_ops: 1,
            base_iv: 9,
        },
        b"\xa3\x01\x02\x04\x01\x05\x09",
    );
}
