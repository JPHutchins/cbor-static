//! R-207: `encoded_len` is the length `to_slice` writes, within `MAX_ENCODED_LEN`, which each type's
//! longest value reaches.

use cbor_static::{Encode, MaxEncodedLen, SelfDescribed, Tagged, encoded_len, to_slice};
use proptest::prelude::*;

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 1024 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// The length `to_slice` writes for `value`, which `encoded_len` agrees with and the type bounds.
fn bounded<T: Encode + MaxEncodedLen>(value: &T) -> usize {
    let mut out = vec![0u8; T::MAX_ENCODED_LEN];
    let written = to_slice(value, &mut out).expect("within MAX_ENCODED_LEN");
    assert_eq!(encoded_len(value), Ok(written));
    written
}

/// `value` is one of its type's longest encodings.
fn widest<T: Encode + MaxEncodedLen>(value: &T) {
    assert_eq!(bounded(value), T::MAX_ENCODED_LEN);
}

#[test]
fn integers_at_their_extremes_are_widest() {
    widest(&u8::MAX);
    widest(&u16::MAX);
    widest(&u32::MAX);
    widest(&u64::MAX);
    widest(&usize::MAX);
    widest(&i8::MIN);
    widest(&i16::MIN);
    widest(&i32::MIN);
    widest(&i64::MIN);
    widest(&isize::MIN);
    widest(&u128::MAX);
    widest(&i128::MIN);
    widest(&i128::MAX);
}

#[test]
fn floats_that_need_their_full_width_are_widest() {
    widest(&1.1_f32);
    widest(&1.1_f64);
    widest(&cbor_static::F16::from_bits(0x3c01));
}

#[test]
fn every_other_leaf_has_a_widest_value() {
    widest(&());
    widest(&true);
    widest(&char::MAX);
    widest(&Some(u32::MAX));
    widest(&None::<()>);
}

#[test]
fn containers_hold_their_widest_elements() {
    widest(&[i64::MIN; 24]);
    widest(&(u8::MAX, i16::MIN, Some(u64::MAX), [true; 2]));
    widest(&Tagged::<{ u64::MAX }, _>(u128::MAX));
    widest(&SelfDescribed(char::MAX));
}

/// The one-shot measurement build moves each item whole, so it counts only items that fit
/// `encoded_len`'s scratch; the resumable build counts any.
#[test]
fn an_item_wider_than_the_scratch() {
    let wide = "x".repeat(100);
    if cfg!(cbor_static_oneshot) {
        assert_eq!(
            encoded_len(&wide.as_str()).map_err(|e| e.kind),
            Err(cbor_static::ErrorKind::BufferTooSmall)
        );
    } else {
        assert_eq!(encoded_len(&wide.as_str()), Ok(102));
    }
}

#[test]
fn a_none_counts_its_payload_as_room() {
    assert_eq!(bounded(&None::<u64>), 1);
    assert_eq!(<Option<u64> as MaxEncodedLen>::MAX_ENCODED_LEN, 9);
}

#[cfg(feature = "heapless")]
#[test]
fn heapless_containers_full_of_their_widest_elements_are_widest() {
    use heapless::{LinearMap, String, Vec};

    widest(&String::<24>::try_from("twenty-four characters!!").expect("fits"));
    widest(&Vec::<i32, 3>::from_array([i32::MIN; 3]));
    widest(
        &[(u16::MAX, char::MAX), (u16::MAX - 1, char::MAX)]
            .into_iter()
            .collect::<LinearMap<u16, char, 2>>(),
    );
    assert_eq!(bounded(&String::<24>::new()), 1);
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn scalars_stay_within_their_bound(
        unsigned in any::<(u8, u16, u32, u64)>(),
        signed in any::<(i8, i16, i32, i64)>(),
        wide in any::<(u128, i128, usize, isize)>(),
        other in any::<(char, bool, Option<u16>)>(),
    ) {
        bounded(&unsigned);
        bounded(&signed);
        bounded(&wide);
        bounded(&other);
    }

    #[test]
    fn floats_stay_within_their_bound(single in any::<u32>(), double in any::<u64>(), half in any::<u16>()) {
        bounded(&f32::from_bits(single));
        bounded(&f64::from_bits(double));
        bounded(&cbor_static::F16::from_bits(half));
    }

    #[test]
    fn containers_stay_within_their_bound(elements in any::<[i16; 5]>(), tagged in any::<u64>()) {
        bounded(&elements);
        bounded(&Tagged::<24, _>(tagged));
        bounded(&SelfDescribed(Some(elements)));
    }
}

#[cfg(feature = "heapless")]
proptest! {
    #![proptest_config(config())]

    #[test]
    fn heapless_containers_stay_within_their_bound(
        text in "\\PC{0,8}",
        elements in proptest::collection::vec(any::<u64>(), 0..=4),
        entries in proptest::collection::vec(any::<(u8, i32)>(), 0..=3),
    ) {
        use heapless::{LinearMap, String, Vec};

        let text: String<32> = String::try_from(text.as_str()).expect("eight characters fit 32 bytes");
        bounded(&text);
        bounded(&Vec::<u64, 4>::from_slice(&elements).expect("fits"));
        bounded(&entries.into_iter().collect::<LinearMap<u8, i32, 3>>());
    }
}
