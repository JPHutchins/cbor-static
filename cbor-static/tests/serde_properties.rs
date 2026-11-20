#![cfg(all(feature = "serde", feature = "alloc"))]
//! D-08: `serde_derive`'s defaults through the serde layer write the native derive's bytes for the
//! same type, and both layers read them back.

use proptest::prelude::*;
use serde::{Deserialize, Serialize};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 512 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Clone, Debug, PartialEq)]
enum Kind {
    Idle,
    Level(u32),
    Span { from: i32, to: i32 },
}

#[derive(Serialize, Deserialize, cbor_static::Cbor, Debug, PartialEq)]
struct Record {
    id: u32,
    delta: i64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    note: Option<u16>,
    name: String,
    samples: Vec<i16>,
    pair: (u8, i8),
    wide: u128,
    signed: i128,
    ratio: f64,
    kind: Kind,
    kinds: [Kind; 2],
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![
        Just(Kind::Idle),
        any::<u32>().prop_map(Kind::Level),
        (any::<i32>(), any::<i32>()).prop_map(|(from, to)| Kind::Span { from, to }),
    ]
}

prop_compose! {
    fn record()(
        (id, delta, ok, note, name, samples) in (
            any::<u32>(),
            any::<i64>(),
            any::<bool>(),
            any::<Option<u16>>(),
            ".{0,24}",
            prop::collection::vec(any::<i16>(), 0..8),
        ),
        (pair, wide, signed, ratio) in (any::<(u8, i8)>(), any::<u128>(), any::<i128>(), any::<u64>()),
        (kind, first, second) in (kind(), kind(), kind()),
    ) -> Record {
        Record {
            id,
            delta,
            ok,
            note,
            name,
            samples,
            pair,
            wide,
            signed,
            ratio: f64::from_bits(ratio),
            kind,
            kinds: [first, second],
        }
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn serde_writes_native_bytes_and_both_read_them(value in record()) {
        let native = cbor_static::to_vec(&value).expect("native encode");
        let serde = cbor_static::serde::to_vec(&value).expect("serde encode");
        prop_assert_eq!(&serde, &native);
        let through_serde = cbor_static::serde::from_slice::<Record>(&native).expect("serde decode");
        let through_native = cbor_static::from_slice::<Record>(&serde).expect("native decode");
        prop_assert_eq!(cbor_static::to_vec(&through_serde).expect("re-encode"), native.clone());
        prop_assert_eq!(cbor_static::to_vec(&through_native).expect("re-encode"), native);
    }
}
