//! R-124 (#13 Q31): `default` on any `T: Default` field, and `default = "path"`, fill a field its
//! container lacks; encoding is unchanged.

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{chunked_decode, decode, golden};

#[derive(Cbor, Debug, PartialEq, Clone, Copy, Default)]
struct Inner {
    x: u32,
}

const fn five() -> u32 {
    5
}

#[expect(clippy::unnecessary_wraps, reason = "a default for an `Option` field")]
const fn some_seven() -> Option<u32> {
    Some(7)
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Config {
    id: u32,
    #[serde(default)]
    retries: u8,
    #[serde(default = "five")]
    timeout: u32,
    #[serde(default)]
    nested: Inner,
    #[serde(default = "some_seven")]
    limit: Option<u32>,
    #[serde(default)]
    flag: Option<u32>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(array)]
struct Point {
    x: u32,
    #[serde(default = "five")]
    y: u32,
}

fn anonymous() -> &'static str {
    "anon"
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Named<'a> {
    #[serde(default = "anonymous")]
    name: &'a str,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
enum Command {
    Move {
        #[serde(default)]
        speed: u32,
        #[serde(default = "five")]
        steps: u32,
        to: i32,
    },
}

const FULL: Config = Config {
    id: 1,
    retries: 2,
    timeout: 3,
    nested: Inner { x: 4 },
    limit: None,
    flag: Some(6),
};

#[test]
fn defaulted_fields_encode_as_any_other() {
    golden(
        &FULL,
        &[
            b"\xa6\x62id\x01\x67retries\x02\x67timeout\x03".as_slice(),
            b"\x66nested\xa1\x61x\x04\x65limit\xf6\x64flag\x06",
        ]
        .concat(),
    );
}

#[test]
fn a_missing_field_takes_its_default_or_its_path() {
    let only_id = b"\xa1\x62id\x01";
    let defaults = Config {
        id: 1,
        retries: 0,
        timeout: 5,
        nested: Inner::default(),
        limit: Some(7),
        flag: None,
    };
    assert_eq!(decode::<Config>(only_id), Ok(defaults));
    for chunk in 1..=only_id.len() {
        assert_eq!(chunked_decode::<Config>(only_id, chunk), Ok(defaults));
    }
    assert_eq!(decode::<Config>(b"\xa0"), Err(ErrorKind::MissingField));
    assert_eq!(decode::<Point>(b"\x81\x01"), Ok(Point { x: 1, y: 5 }));
    assert_eq!(decode::<Point>(b"\x80"), Err(ErrorKind::MissingField));
    golden(&Point { x: 1, y: 2 }, b"\x82\x01\x02");
    assert_eq!(decode::<Named>(b"\xa0"), Ok(Named { name: "anon" }));
    assert_eq!(
        decode::<Command>(b"\xa1\x64Move\xa1\x62to\x20"),
        Ok(Command::Move {
            speed: 0,
            steps: 5,
            to: -1
        })
    );
}

#[test]
fn a_present_field_decodes_as_its_type_does() {
    assert_eq!(
        decode::<Config>(b"\xa2\x62id\x01\x66nested\xa1\x61x\x09"),
        Ok(Config {
            nested: Inner { x: 9 },
            ..Config {
                id: 1,
                retries: 0,
                timeout: 5,
                nested: Inner::default(),
                limit: Some(7),
                flag: None,
            }
        })
    );
    assert_eq!(
        decode::<Config>(b"\xa2\x62id\x01\x67timeout\x61x"),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(
        decode::<Config>(b"\xa2\x62id\x01\x65limit\xf6"),
        Ok(Config {
            id: 1,
            retries: 0,
            timeout: 5,
            nested: Inner::default(),
            limit: None,
            flag: None,
        })
    );
}
