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

/// Neither encodes nor decodes: only a `skip`ped field may have it.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
struct Cache(u32);

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Texted {
    #[serde(skip)]
    cache: Cache,
    id: u32,
    #[serde(skip, default = "five")]
    hits: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deny_unknown_fields)]
struct Denied {
    #[serde(skip)]
    cache: Cache,
    id: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Keyed {
    #[serde(skip)]
    cache: Cache,
    #[cbor(key = 1)]
    alg: i32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Mixed {
    #[serde(skip)]
    cache: Cache,
    #[cbor(key = 1)]
    alg: i32,
    name: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(array)]
struct Row {
    #[serde(skip)]
    cache: Cache,
    x: u32,
    y: Option<u32>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
enum Event {
    Hit {
        #[serde(skip)]
        cache: Cache,
        at: u32,
    },
}

#[test]
fn a_skipped_field_is_never_written_and_decodes_to_its_default() {
    golden(
        &Texted {
            cache: Cache(0),
            id: 1,
            hits: 5,
        },
        b"\xa1\x62id\x01",
    );
    assert_eq!(
        decode::<Texted>(b"\xa2\x65cache\x09\x62id\x01"),
        Ok(Texted {
            cache: Cache(0),
            id: 1,
            hits: 5,
        }),
        "an entry named like a skipped field is unknown"
    );
    assert_eq!(
        decode::<Texted>(b"\xa2\x61\xff\x00\x62id\x01"),
        Err(ErrorKind::InvalidUtf8),
        "the skipped field's sentinel name is no key"
    );
    assert_eq!(
        decode::<Denied>(b"\xa2\x65cache\x09\x62id\x01"),
        Err(ErrorKind::UnknownField)
    );
    golden(
        &Keyed {
            cache: Cache(0),
            alg: -7,
        },
        b"\xa1\x01\x26",
    );
    golden(
        &Mixed {
            cache: Cache(0),
            alg: -7,
            name: 3,
        },
        b"\xa2\x01\x26\x64name\x03",
    );
    golden(
        &Row {
            cache: Cache(0),
            x: 1,
            y: Some(2),
        },
        b"\x82\x01\x02",
    );
    golden(
        &Row {
            cache: Cache(0),
            x: 1,
            y: None,
        },
        b"\x81\x01",
    );
    assert_eq!(
        decode::<Row>(b"\x83\x01\x02\x03"),
        Err(ErrorKind::TypeMismatch),
        "no element reaches a skipped field"
    );
    golden(
        &Event::Hit {
            cache: Cache(0),
            at: 4,
        },
        b"\xa1\x63Hit\xa1\x62at\x04",
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(transparent)]
struct Meters {
    value: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(transparent)]
struct Digest(#[cbor(bytes)] [u8; 2]);

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(transparent)]
struct Label<'a> {
    #[serde(borrow)]
    text: &'a str,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Borrowing<'a> {
    #[serde(borrow = "'a")]
    name: &'a str,
    count: u32,
}

#[test]
fn a_transparent_struct_is_its_one_field() {
    golden(&Meters { value: 1000 }, b"\x19\x03\xe8");
    golden(&Digest([1, 2]), b"\x42\x01\x02");
    assert_eq!(decode::<Label>(b"\x62hi"), Ok(Label { text: "hi" }));
    assert_eq!(
        decode::<Borrowing>(b"\xa2\x64name\x61a\x65count\x01"),
        Ok(Borrowing {
            name: "a",
            count: 1
        })
    );
}
