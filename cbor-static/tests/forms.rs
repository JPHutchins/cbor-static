//! R-123 and R-124 (#13) as properties: every value of the 1d wire forms round-trips whole and in
//! arbitrary chunks, and decoding fills what a map lacks with exactly the field's default.

mod common;

use cbor_static::{Cbor, Tagged};
use common::{chunked_decode, chunked_encode, decode, encode};
use proptest::prelude::*;

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 512 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
#[cbor(index)]
enum Indexed {
    Ping = 1,
    Data(u32) = 5,
    Ack,
    Close { code: u16, reason: Option<i8> } = 200,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(index)]
enum Signed {
    Low = -3,
    Mid,
    High = 7,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
enum Stamp {
    Clock(Tagged<0, u32>),
    Epoch(Tagged<1, u64>),
    Mac(Tagged<97, i32>),
    Sign(Tagged<98, u32>),
    Bare(u64),
    Missing,
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
struct Cache(u32);

const fn seven() -> u32 {
    7
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct Settings {
    id_number: u32,
    #[serde(default)]
    retry_count: u8,
    #[serde(default = "seven")]
    time_out: u32,
    #[serde(skip)]
    cache: Cache,
    #[serde(skip, default = "seven")]
    hits: u32,
}

fn indexed() -> impl Strategy<Value = Indexed> {
    prop_oneof![
        Just(Indexed::Ping),
        any::<u32>().prop_map(Indexed::Data),
        Just(Indexed::Ack),
        (any::<u16>(), any::<Option<i8>>())
            .prop_map(|(code, reason)| Indexed::Close { code, reason }),
    ]
}

fn stamp() -> impl Strategy<Value = Stamp> {
    prop_oneof![
        any::<u32>().prop_map(|value| Stamp::Clock(Tagged(value))),
        any::<u64>().prop_map(|value| Stamp::Epoch(Tagged(value))),
        any::<i32>().prop_map(|value| Stamp::Mac(Tagged(value))),
        any::<u32>().prop_map(|value| Stamp::Sign(Tagged(value))),
        any::<u64>().prop_map(Stamp::Bare),
        Just(Stamp::Missing),
    ]
}

/// `value` encodes the same whole and in `chunk`-byte pieces, and decodes back both ways; returns
/// the encoding.
fn round_trips<T>(value: T, chunk: usize) -> Vec<u8>
where
    T: cbor_static::Encode
        + for<'de> cbor_static::Decode<'de>
        + Copy
        + PartialEq
        + core::fmt::Debug,
{
    let bytes = encode(&value);
    assert_eq!(chunked_encode(&value, chunk), bytes);
    assert_eq!(decode::<T>(&bytes), Ok(value));
    assert_eq!(chunked_decode::<T>(&bytes, chunk), Ok(value));
    bytes
}

/// A text-keyed map entry: the pair's encoding without its array head.
fn entry<V: cbor_static::Encode>(key: &str, value: V) -> Vec<u8> {
    encode(&(key, value)).split_off(1)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn index_enums_round_trip_and_write_units_bare(value in indexed(), chunk in 1usize..16) {
        let bytes = round_trips(value, chunk);
        let unit = matches!(value, Indexed::Ping | Indexed::Ack);
        prop_assert_eq!(bytes[0] >> 5 == 4, !unit, "{:x?}", bytes);
    }

    #[test]
    fn signed_discriminants_round_trip(value in prop_oneof![Just(Signed::Low), Just(Signed::Mid), Just(Signed::High)], chunk in 1usize..4) {
        let bytes = round_trips(value, chunk);
        let expected = value as i64;
        prop_assert_eq!(
            bytes,
            if expected < 0 { vec![0x20 | u8::try_from(!expected).expect("small")] } else { vec![u8::try_from(expected).expect("small")] }
        );
    }

    #[test]
    fn untagged_tag_numbers_pick_their_variant(value in stamp(), chunk in 1usize..16) {
        round_trips(value, chunk);
    }

    #[test]
    fn a_map_lacking_fields_decodes_their_defaults(
        id_number in any::<u32>(),
        retry_count in proptest::option::of(any::<u8>()),
        time_out in proptest::option::of(any::<u32>()),
        cache in any::<u32>(),
        hits in any::<u32>(),
    ) {
        let written = Settings { id_number, retry_count: retry_count.unwrap_or(0), time_out: time_out.unwrap_or(7), cache: Cache(cache), hits };
        let full = encode(&written);
        prop_assert_eq!(
            decode::<Settings>(&full),
            Ok(Settings { cache: Cache::default(), hits: 7, ..written })
        );
        let entries: Vec<Vec<u8>> = [
            Some(entry("idNumber", id_number)),
            retry_count.map(|count| entry("retryCount", count)),
            time_out.map(|out| entry("timeOut", out)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let partial = [vec![0xa0 | u8::try_from(entries.len()).expect("few")], entries.concat()].concat();
        prop_assert_eq!(
            decode::<Settings>(&partial),
            Ok(Settings { cache: Cache::default(), hits: 7, ..written })
        );
    }
}
