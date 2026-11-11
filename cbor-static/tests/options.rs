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
enum Shape {
    Dot,
    Circle(u32),
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Feature {
    at: Option<Coord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shape: Option<Shape>,
    points: Option<[u32; 2]>,
    pair: Option<(bool, u32)>,
}

const FEATURE: Feature = Feature {
    at: Some(Coord { x: 1, y: -1 }),
    shape: Some(Shape::Circle(5)),
    points: Some([1, 2]),
    pair: Some((true, 3)),
};

#[test]
fn a_nested_option_is_null_or_its_payload() {
    golden(
        &FEATURE,
        &[
            b"\xa4\x62at\xa2\x61x\x01\x61y\x20".as_slice(),
            b"\x65shape\xa1\x66Circle\x05",
            b"\x66points\x82\x01\x02\x64pair\x82\xf5\x03",
        ]
        .concat(),
    );
    golden(
        &Feature {
            at: None,
            shape: None,
            points: None,
            pair: None,
        },
        b"\xa3\x62at\xf6\x66points\xf6\x64pair\xf6",
    );
    golden(
        &Feature {
            shape: Some(Shape::Dot),
            ..FEATURE
        },
        &[
            b"\xa4\x62at\xa2\x61x\x01\x61y\x20".as_slice(),
            b"\x65shape\x63Dot",
            b"\x66points\x82\x01\x02\x64pair\x82\xf5\x03",
        ]
        .concat(),
    );
    golden(&Some(Coord { x: 0, y: 0 }), b"\xa2\x61x\x00\x61y\x00");
    golden(&None::<Coord>, b"\xf6");
}

#[test]
fn a_missing_nested_option_decodes_as_none() {
    assert_eq!(
        decode::<Feature>(b"\xa1\x62at\xa2\x61x\x01\x61y\x20"),
        Ok(Feature {
            at: FEATURE.at,
            shape: None,
            points: None,
            pair: None,
        })
    );
    assert_eq!(
        decode::<Option<Coord>>(b"\xa1\x61x\x01"),
        Err(ErrorKind::MissingField)
    );
}
