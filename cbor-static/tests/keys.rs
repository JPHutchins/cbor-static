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
