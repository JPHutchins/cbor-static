#![cfg(all(feature = "serde", feature = "alloc"))]
//! R-302's attribute corpus: one type per serde attribute the native derive honours, deriving both
//! layers from one definition (D-08, D-08a). Each value encodes to the same bytes through both, and
//! each layer's decode of them is the value, which the other layer re-encodes to the same bytes.

use core::fmt::Debug;

use cbor_static::{Decode, Encode};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 4 } else { 256 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// `value`'s bytes, the same through both layers.
fn encoded<T: Serialize + Encode>(value: &T) -> Vec<u8> {
    let native = cbor_static::to_vec(value).expect("native encode");
    assert_eq!(
        cbor_static::serde::to_vec(value).as_ref(),
        Ok(&native),
        "serde encode"
    );
    native
}

/// `bytes` decodes to `expected` through each layer, and re-encodes to `bytes` through the other.
fn decoded<'de, T>(bytes: &'de [u8], expected: &T)
where
    T: Serialize + Encode + Deserialize<'de> + Decode<'de> + PartialEq + Debug,
{
    let native = cbor_static::from_slice::<T>(bytes).expect("native decode");
    assert_eq!(&native, expected, "native decode");
    assert_eq!(cbor_static::serde::to_vec(&native).as_deref(), Ok(bytes));
    let serde = cbor_static::serde::from_slice::<T>(bytes).expect("serde decode");
    assert_eq!(&serde, expected, "serde decode");
    assert_eq!(cbor_static::to_vec(&serde).as_deref(), Ok(bytes));
}

/// `value` through both layers both ways.
fn alike<T>(value: &T)
where
    T: Serialize + Encode + for<'de> Deserialize<'de> + for<'de> Decode<'de> + PartialEq + Debug,
{
    decoded(&encoded(value), value);
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Renamed {
    #[serde(rename = "ID")]
    id: u32,
    name: String,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Camel {
    key_ops: u32,
    alg_id: i32,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
struct Pascal {
    key_ops: u32,
    base_iv: bool,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
struct Kebab {
    key_ops: u32,
    write_through: bool,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Snake {
    ReadAll,
    WriteThrough(u8),
    SeekTo { at_offset: u32 },
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Defaulted {
    id: u32,
    #[serde(default)]
    level: u16,
}

/// [`Defaulted`] without its defaulted field.
#[derive(Serialize, cbor_static::Encode)]
struct IdOnly {
    id: u32,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Skipped {
    id: u32,
    #[serde(skip)]
    cache: u32,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct SkippedIfNone {
    id: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    note: Option<u16>,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Inner {
    x: u16,
    y: bool,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Flattened {
    id: u8,
    #[serde(flatten)]
    inner: Inner,
    last: i8,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Clone, Debug, PartialEq)]
#[serde(tag = "type")]
enum Internal {
    Circle { r: u32 },
    Square { side: u16, filled: bool },
    Empty,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Clone, Debug, PartialEq)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Level(u32),
    Span { from: i16, to: i16 },
    Idle,
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Clone, Debug, PartialEq)]
#[serde(untagged)]
enum Untagged {
    Number(u64),
    Name(String),
    Flags([bool; 2]),
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Borrowed<'a> {
    #[serde(borrow)]
    name: &'a str,
    #[serde(borrow, with = "serde_bytes")]
    tag: &'a [u8],
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
#[serde(transparent)]
struct Meters {
    value: u32,
}

fn snake() -> impl Strategy<Value = Snake> {
    prop_oneof![
        Just(Snake::ReadAll),
        any::<u8>().prop_map(Snake::WriteThrough),
        any::<u32>().prop_map(|at_offset| Snake::SeekTo { at_offset }),
    ]
}

fn internal() -> impl Strategy<Value = Internal> {
    prop_oneof![
        any::<u32>().prop_map(|r| Internal::Circle { r }),
        any::<(u16, bool)>().prop_map(|(side, filled)| Internal::Square { side, filled }),
        Just(Internal::Empty),
    ]
}

fn adjacent() -> impl Strategy<Value = Adjacent> {
    prop_oneof![
        any::<u32>().prop_map(Adjacent::Level),
        any::<(i16, i16)>().prop_map(|(from, to)| Adjacent::Span { from, to }),
        Just(Adjacent::Idle),
    ]
}

fn untagged_values() -> impl Strategy<Value = Untagged> {
    prop_oneof![
        any::<u64>().prop_map(Untagged::Number),
        ".{0,30}".prop_map(Untagged::Name),
        any::<[bool; 2]>().prop_map(Untagged::Flags),
    ]
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn rename(id: u32, name in ".{0,30}") {
        alike(&Renamed { id, name });
    }

    #[test]
    fn rename_all(key_ops: u32, alg_id: i32, flag: bool, variant in snake()) {
        alike(&Camel { key_ops, alg_id });
        alike(&Pascal { key_ops, base_iv: flag });
        alike(&Kebab { key_ops, write_through: flag });
        alike(&variant);
    }

    #[test]
    fn default(id: u32, level: u16) {
        alike(&Defaulted { id, level });
        let without_level = encoded(&IdOnly { id });
        prop_assert_eq!(
            cbor_static::from_slice::<Defaulted>(&without_level),
            Ok(Defaulted { id, level: 0 })
        );
        prop_assert_eq!(
            cbor_static::serde::from_slice::<Defaulted>(&without_level),
            Ok(Defaulted { id, level: 0 })
        );
    }

    #[test]
    fn skip(id: u32, cache: u32) {
        let bytes = encoded(&Skipped { id, cache });
        decoded(&bytes, &Skipped { id, cache: 0 });
    }

    #[test]
    fn skip_serializing_if(id: u8, note: Option<u16>) {
        alike(&SkippedIfNone { id, note });
    }

    #[test]
    fn flatten(id: u8, x: u16, y: bool, last: i8) {
        alike(&Flattened { id, inner: Inner { x, y }, last });
    }

    #[test]
    fn tag(value in internal()) {
        alike(&value);
    }

    #[test]
    fn content(value in adjacent()) {
        alike(&value);
    }

    #[test]
    fn untagged(value in untagged_values()) {
        alike(&value);
    }

    #[test]
    fn borrow(name in ".{0,30}", tag: Vec<u8>) {
        let value = Borrowed { name: &name, tag: &tag };
        decoded(&encoded(&value), &value);
    }

    #[test]
    fn transparent(value: u32) {
        alike(&Meters { value });
    }
}
