//! #5 Q2: `#[cbor(max_encoded_len)]` derives `MaxEncodedLen`, which each form's widest value
//! reaches and every value stays within.

use cbor_static::{Encode, MaxEncodedLen, encode_into, encoded_len, to_slice};

/// The length `to_slice` writes, which `encoded_len` agrees with and the type bounds.
fn bounded<T: Encode + MaxEncodedLen>(value: &T) -> usize {
    let mut out = vec![0u8; T::MAX_ENCODED_LEN];
    let written = to_slice(value, &mut out).expect("within MAX_ENCODED_LEN");
    assert_eq!(encoded_len(value), Ok(written));
    written
}

fn widest<T: Encode + MaxEncodedLen>(value: &T) {
    assert_eq!(bounded(value), T::MAX_ENCODED_LEN);
}

#[derive(Clone, Copy)]
enum Label {
    Alg = 1,
    Crv = -100,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct TextKeys {
    id: u32,
    #[serde(rename = "a longer name")]
    named: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    maybe: Option<u8>,
    #[serde(skip)]
    skipped: u64,
}

#[derive(cbor_static::Encode)]
#[cbor(keys = Label, max_encoded_len)]
struct LabelKeys {
    alg: i64,
    crv: [bool; 2],
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct MixedKeys {
    #[cbor(key = -1000)]
    negative: u8,
    #[cbor(key = "t")]
    text: char,
}

#[test]
fn map_structs_count_keys_and_values() {
    widest(&TextKeys {
        id: u32::MAX,
        named: i16::MIN,
        maybe: Some(u8::MAX),
        skipped: 0,
    });
    assert!(
        bounded(&TextKeys {
            id: 0,
            named: 0,
            maybe: None,
            skipped: u64::MAX,
        }) < TextKeys::MAX_ENCODED_LEN
    );
    widest(&LabelKeys {
        alg: i64::MIN,
        crv: [true; 2],
    });
    widest(&MixedKeys {
        negative: u8::MAX,
        text: char::MAX,
    });
}

#[derive(cbor_static::Encode)]
#[cbor(array, max_encoded_len)]
struct Positional {
    at: (i32, i32),
    label: Option<u64>,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Pair(u16, Option<i8>);

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Meters(u64);

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Ping;

#[derive(cbor_static::Encode)]
#[serde(transparent)]
#[cbor(max_encoded_len)]
struct Wrapped {
    inner: Pair,
}

#[test]
fn positional_and_wrapping_structs_count_their_fields() {
    widest(&Positional {
        at: (i32::MIN, i32::MIN),
        label: Some(u64::MAX),
    });
    widest(&Pair(u16::MAX, Some(i8::MIN)));
    widest(&Meters(u64::MAX));
    widest(&Ping);
    widest(&Wrapped {
        inner: Pair(u16::MAX, Some(i8::MIN)),
    });
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Generic<T> {
    value: T,
    values: [T; 3],
}

#[test]
fn generic_types_forward_their_parameters_bound() {
    widest(&Generic {
        value: i64::MIN,
        values: [i64::MIN; 3],
    });
    widest(&Generic {
        value: Ping,
        values: [Ping, Ping, Ping],
    });
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Inner {
    x: u32,
    y: Option<u32>,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Flattening {
    id: u8,
    #[serde(flatten)]
    inner: Inner,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Meta<T> {
    tag: T,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct GenericFlattening<T> {
    id: u8,
    #[serde(flatten)]
    meta: Meta<T>,
}

#[derive(cbor_static::Encode)]
#[serde(tag = "type")]
#[cbor(max_encoded_len)]
enum GenericInternal<T> {
    Held(Meta<T>),
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Counting {
    id: u8,
    #[cbor(other)]
    unknown: cbor_static::Skipped,
}

#[test]
fn flattened_entries_join_the_map_and_skipped_ones_add_none() {
    widest(&Flattening {
        id: u8::MAX,
        inner: Inner {
            x: u32::MAX,
            y: Some(u32::MAX),
        },
    });
    widest(&GenericFlattening {
        id: u8::MAX,
        meta: Meta { tag: i64::MIN },
    });
    widest(&GenericInternal::Held(Meta { tag: i64::MIN }));
    widest(&Counting {
        id: u8::MAX,
        unknown: cbor_static::Skipped::default(),
    });
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Bounded<'a> {
    #[cbor(bytes)]
    key: [u8; 30],
    #[cbor(max_len = 300)]
    name: &'a str,
    #[cbor(max_len = 2)]
    raw: Option<&'a [u8]>,
}

#[test]
fn byte_strings_and_max_len_fields_count_their_limit() {
    widest(&Bounded {
        key: [0xff; 30],
        name: &"x".repeat(300),
        raw: Some(&[1, 2]),
    });
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
enum External {
    Unit,
    Newtype(u64),
    Struct { a: i32, b: Option<u16> },
}

#[derive(cbor_static::Encode)]
#[cbor(index, max_encoded_len)]
enum Index {
    Unit,
    Payload(u8),
}

#[derive(cbor_static::Encode)]
#[cbor(index, max_encoded_len)]
enum Wide {
    Small = 1,
    Large = 1000,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
enum Tags {
    #[cbor(cbor_tag = 18)]
    Signed(u32),
    #[cbor(cbor_tag = 70_000)]
    Maced(u32),
}

#[derive(cbor_static::Encode)]
#[serde(tag = "type")]
#[cbor(max_encoded_len)]
enum Internal {
    Unit,
    Struct { x: i64 },
    Newtype(Inner),
}

#[derive(cbor_static::Encode)]
#[serde(tag = "t", content = "c")]
#[cbor(max_encoded_len)]
enum Adjacent {
    Unit,
    Data(u32),
}

#[derive(cbor_static::Encode)]
#[serde(untagged)]
#[cbor(max_encoded_len)]
enum Untagged {
    Unit,
    Number(u64),
}

#[test]
fn enums_count_their_widest_variant() {
    widest(&External::Struct {
        a: i32::MIN,
        b: Some(u16::MAX),
    });
    bounded(&External::Unit);
    bounded(&External::Newtype(u64::MAX));
    widest(&Index::Payload(u8::MAX));
    bounded(&Index::Unit);
    widest(&Wide::Large);
    bounded(&Wide::Small);
    widest(&Tags::Maced(u32::MAX));
    bounded(&Tags::Signed(u32::MAX));
    widest(&Internal::Newtype(Inner {
        x: u32::MAX,
        y: Some(u32::MAX),
    }));
    bounded(&Internal::Unit);
    bounded(&Internal::Struct { x: i64::MIN });
    widest(&Adjacent::Data(u32::MAX));
    bounded(&Adjacent::Unit);
    widest(&Untagged::Number(u64::MAX));
    bounded(&Untagged::Unit);
}

#[test]
fn each_variant_is_widest_when_alone() {
    #[derive(cbor_static::Encode)]
    #[serde(tag = "type")]
    #[cbor(max_encoded_len)]
    enum UnitOnly {
        Unit,
    }

    #[derive(cbor_static::Encode)]
    #[serde(tag = "type")]
    #[cbor(max_encoded_len)]
    enum StructOnly {
        Struct { x: i64 },
    }

    #[derive(cbor_static::Encode)]
    #[serde(tag = "t", content = "c")]
    #[cbor(max_encoded_len)]
    enum AdjacentUnit {
        Unit,
    }

    #[derive(cbor_static::Encode)]
    #[cbor(max_encoded_len)]
    enum ExternalNewtype {
        Newtype(u64),
    }

    widest(&UnitOnly::Unit);
    widest(&StructOnly::Struct { x: i64::MIN });
    widest(&AdjacentUnit::Unit);
    widest(&ExternalNewtype::Newtype(u64::MAX));
}

#[derive(cbor_static::Encode, cbor_static::Decode)]
#[cbor(max_encoded_len)]
struct EncodeFirst {
    a: u8,
}

#[derive(cbor_static::Decode, cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct DecodeFirst {
    a: u8,
}

#[derive(cbor_static::Cbor)]
#[cbor(max_encoded_len)]
struct Both {
    a: u8,
}

#[test]
fn either_derive_order_implements_it_once() {
    widest(&EncodeFirst { a: u8::MAX });
    widest(&DecodeFirst { a: u8::MAX });
    widest(&Both { a: u8::MAX });
}

#[test]
fn encode_into_takes_a_buffer_of_the_bound() {
    let mut out = [0u8; TextKeys::MAX_ENCODED_LEN];
    let value = TextKeys {
        id: 7,
        named: -1,
        maybe: None,
        skipped: 0,
    };
    let len = encode_into(&value, &mut out).expect("fits");
    assert_eq!(&out[..len], b"\xa2\x62id\x07\x6da longer name\x20");
}

#[cfg(feature = "heapless")]
mod bounded {
    use heapless::{LinearMap, String, Vec};

    use super::widest;

    #[derive(cbor_static::Encode)]
    #[cbor(max_encoded_len)]
    struct Owned {
        name: String<30>,
        #[cbor(bytes)]
        raw: Vec<u8, 300>,
        #[cbor(max_len = 2)]
        values: Vec<u32, 8>,
        #[cbor(bytes, max_len = 24)]
        short: Vec<u8, 64>,
        map: LinearMap<u8, i8, 2>,
    }

    #[test]
    fn heapless_fields_count_their_capacity_or_limit() {
        widest(&Owned {
            name: String::try_from("x".repeat(30).as_str()).expect("fits"),
            raw: Vec::from_slice(&[0; 300]).expect("fits"),
            values: Vec::from_slice(&[u32::MAX; 2]).expect("fits"),
            short: Vec::from_slice(&[0; 24]).expect("fits"),
            map: [(u8::MAX, i8::MIN), (u8::MAX - 1, i8::MIN)]
                .into_iter()
                .collect(),
        });
    }
}
