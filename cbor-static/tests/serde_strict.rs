#![cfg(feature = "serde")]
//! `Deserializer::new_strict` (#6 Q4) refuses what the native Strict decoder refuses, with the same
//! kind at the same offset: non-deterministic heads, floats and bignums, indefinite lengths and
//! map keys out of order as `NotDeterministic`, and an entry no field claims as `UnknownField`.

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::ErrorKind;
use cbor_static::serde::Deserializer;
use serde::Deserialize;

fn serde_strict<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, (ErrorKind, u32)> {
    Deserializer::new_strict(bytes)
        .decode()
        .map_err(|e| (e.kind, e.offset))
}

/// `bytes` decodes alike through both layers' Strict, or is refused alike: `outcome`.
fn alike<'de, T>(bytes: &'de [u8], outcome: Result<(), ErrorKind>)
where
    T: Deserialize<'de> + cbor_static::Decode<'de> + Debug,
{
    let mut slot = MaybeUninit::uninit();
    let native = cbor_static::decode_strict_into::<T>(bytes, &mut slot)
        .map(|_| ())
        .map_err(|e| (e.kind, e.offset));
    assert_eq!(
        native.map_err(|(kind, _)| kind),
        outcome,
        "native {bytes:02x?}"
    );
    assert_eq!(
        serde_strict::<T>(bytes).map(|_| ()),
        native,
        "serde {bytes:02x?}"
    );
}

#[derive(Deserialize, cbor_static::Decode, Debug, PartialEq)]
struct Pair {
    a: u32,
    b: u32,
}

#[derive(Deserialize, cbor_static::Decode, Debug)]
struct Float {
    f: f64,
}

#[derive(Deserialize, cbor_static::Decode, Debug)]
struct Wide {
    w: u128,
}

#[derive(Deserialize, cbor_static::Decode, Debug)]
struct List {
    s: [u8; 2],
}

const NOT: Result<(), ErrorKind> = Err(ErrorKind::NotDeterministic);

#[test]
fn deterministic_input_decodes() {
    alike::<Pair>(b"\xa2\x61a\x01\x61b\x02", Ok(()));
    alike::<Float>(b"\xa1\x61f\xf9\x3c\x00", Ok(()));
    alike::<Wide>(b"\xa1\x61w\xc2\x49\x01\0\0\0\0\0\0\0\0", Ok(()));
    alike::<List>(b"\xa1\x61s\x82\x01\x02", Ok(()));
    assert_eq!(
        serde_strict::<Pair>(b"\xa2\x61a\x01\x61b\x02"),
        Ok(Pair { a: 1, b: 2 })
    );
}

#[test]
fn heads_take_their_preferred_length() {
    alike::<Pair>(b"\xa2\x61a\x18\x01\x61b\x02", NOT);
    alike::<Pair>(b"\xb8\x02\x61a\x01\x61b\x02", NOT);
    alike::<Pair>(b"\xa2\x78\x01a\x01\x61b\x02", NOT);
    alike::<Wide>(b"\xa1\x61w\xd8\x02\x49\x01\0\0\0\0\0\0\0\0", NOT);
}

#[test]
fn floats_take_their_shortest_exact_width() {
    alike::<Float>(b"\xa1\x61f\xfb\x3f\xf0\0\0\0\0\0\0", NOT);
    alike::<Float>(b"\xa1\x61f\xfa\x3f\x80\x00\x00", NOT);
}

#[test]
fn bignums_take_only_the_digits_they_need_past_a_u64() {
    alike::<Wide>(b"\xa1\x61w\xc2\x41\x01", NOT);
    alike::<Wide>(b"\xa1\x61w\xc2\x49\x00\x01\0\0\0\0\0\0\0", NOT);
}

#[test]
fn lengths_are_definite() {
    alike::<Pair>(b"\xbf\x61a\x01\x61b\x02\xff", NOT);
    alike::<List>(b"\xa1\x61s\x9f\x01\x02\xff", NOT);
    alike::<Pair>(b"\xa2\x61a\x01\x7f\x61b\xff\x02", NOT);
}

#[test]
fn keys_rise_bytewise_and_a_duplicate_is_one() {
    alike::<Pair>(b"\xa2\x61b\x02\x61a\x01", NOT);
    alike::<Pair>(b"\xa2\x61a\x01\x61a\x02", Err(ErrorKind::DuplicateKey));
}

#[test]
fn an_entry_no_field_claims_is_unknown_even_out_of_order() {
    let unknown = Err(ErrorKind::UnknownField);
    alike::<Pair>(b"\xa3\x61a\x01\x61b\x02\x61c\x03", unknown);
    alike::<Pair>(b"\xa3\x61a\x01\x61\x41\x00\x61b\x02", unknown);
    alike::<Pair>(b"\xa3\x61a\x01\x61b\x02\x01\x00", unknown);
    alike::<Pair>(b"\xa3\x61a\x01\x61b\x02\x61c\x9f\xff", unknown);
}

/// A key that is not text is skipped unread, but under Strict its heads are checked first.
#[test]
fn a_non_text_key_takes_its_preferred_head() {
    alike::<Pair>(b"\xa3\x61a\x01\x61b\x02\x18\x01\x00", NOT);
}

#[derive(Deserialize, cbor_static::Decode, Debug)]
struct Nested {
    a: u8,
    m: Pair,
}

#[derive(Deserialize, cbor_static::Decode, Debug)]
struct Last {
    a: u8,
    z: u8,
}

/// A key out of order is refused whatever reads the value after it.
#[test]
fn a_key_out_of_order_is_refused_whatever_reads_its_value() {
    alike::<Nested>(b"\xa2\x61m\xa2\x61a\x01\x61b\x02\x61a\x00", NOT);
    alike::<Last>(b"\xa2\x61z\x00\x61a\x01", NOT);
    assert_eq!(
        serde_strict::<Last>(b"\xa2\x61z\x00\x61a\x01").map(|_| ()),
        Err((ErrorKind::NotDeterministic, 6))
    );
    assert_eq!(
        serde_strict::<Nested>(b"\xa2\x61m\xa2\x61a\x01\x61b\x02\x61a\x00").map(|_| ()),
        Err((ErrorKind::NotDeterministic, 12))
    );
}

#[cfg(feature = "alloc")]
#[test]
fn a_key_out_of_order_is_refused_when_any_reads_its_value() {
    #[derive(Deserialize, Debug)]
    #[serde(untagged)]
    #[allow(dead_code)]
    enum Either {
        Number(u8),
        Name(String),
    }

    #[derive(Deserialize, Debug)]
    #[allow(dead_code)]
    struct Holder {
        a: u8,
        e: Either,
    }

    assert_eq!(
        serde_strict::<Holder>(b"\xa2\x61e\x61x\x61a\x01").map(|_| ()),
        Err((ErrorKind::NotDeterministic, 7))
    );
    assert_eq!(
        serde_strict::<std::collections::BTreeMap<u8, Either>>(b"\xa2\x01\x00\x00\x61x")
            .map(|_| ()),
        Err((ErrorKind::NotDeterministic, 4))
    );
}

/// A key out of order is refused even when it is a map itself, whose own keys rise.
#[cfg(feature = "alloc")]
#[test]
fn a_key_out_of_order_that_is_a_map_is_refused() {
    use std::collections::BTreeMap;

    assert_eq!(
        serde_strict::<BTreeMap<BTreeMap<u8, u8>, u8>>(b"\xa2\xa1\x01\x00\x00\xa1\x00\x00\x00")
            .map(|_| ()),
        Err((ErrorKind::NotDeterministic, 8))
    );
}

#[cfg(feature = "alloc")]
#[test]
fn a_map_s_keys_rise_bytewise_too() {
    let map =
        |bytes| serde_strict::<std::collections::BTreeMap<i32, u8>>(bytes).map(|map| map.len());
    assert_eq!(map(b"\xa3\x00\x00\x01\x00\x20\x00"), Ok(3));
    assert_eq!(
        map(b"\xa2\x20\x00\x00\x00"),
        Err((ErrorKind::NotDeterministic, 4))
    );
    assert_eq!(
        map(b"\xa2\x00\x00\x00\x01"),
        Err((ErrorKind::NotDeterministic, 4))
    );
}

#[test]
fn serde_s_ignored_any_is_an_entry_no_field_claims() {
    #[derive(Deserialize, Debug)]
    #[allow(dead_code)]
    struct Claimed {
        a: serde::de::IgnoredAny,
    }

    assert!(
        Deserializer::new(b"\xa1\x61a\x01")
            .decode::<Claimed>()
            .is_ok()
    );
    assert_eq!(
        serde_strict::<Claimed>(b"\xa1\x61a\x01").map(|_| ()),
        Err((ErrorKind::UnknownField, 3))
    );
}

#[test]
fn a_serde_struct_declared_out_of_bytewise_order_refuses_its_own_bytes() {
    #[derive(serde::Serialize, Deserialize, Debug)]
    struct Declared {
        b: u8,
        a: u8,
    }

    let mut out = [0u8; 16];
    let len = cbor_static::serde::to_slice(&Declared { b: 1, a: 2 }, &mut out).expect("encodes");
    assert!(Deserializer::new(&out[..len]).decode::<Declared>().is_ok());
    assert_eq!(
        serde_strict::<Declared>(&out[..len]).map(|_| ()),
        Err((ErrorKind::NotDeterministic, 6))
    );
}

#[cfg(feature = "alloc")]
mod buffered {
    use cbor_static::ErrorKind;
    use serde::Deserialize;

    use super::serde_strict;

    #[derive(Deserialize, Debug, PartialEq)]
    struct Inner {
        x: u8,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct Outer {
        a: u8,
        #[serde(flatten)]
        inner: Inner,
    }

    /// serde decodes a flattened struct's entries with its own code (D-10c): Strict still checks
    /// every head and key order while serde buffers them, but an entry no field claims passes.
    #[test]
    fn inside_serde_s_buffered_forms_only_unknown_fields_pass() {
        assert_eq!(
            serde_strict::<Outer>(b"\xa3\x61a\x01\x61x\x02\x61y\x03"),
            Ok(Outer {
                a: 1,
                inner: Inner { x: 2 }
            })
        );
        assert_eq!(
            serde_strict::<Outer>(b"\xa2\x61a\x01\x61x\x18\x02").map_err(|(kind, _)| kind),
            Err(ErrorKind::NotDeterministic)
        );
        assert_eq!(
            serde_strict::<Outer>(b"\xa2\x61x\x02\x61a\x01").map_err(|(kind, _)| kind),
            Err(ErrorKind::NotDeterministic)
        );
    }
}
