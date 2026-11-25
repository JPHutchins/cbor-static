#![forbid(unsafe_code)]
#![cfg_attr(feature = "nightly", feature(f16))]
//! Decoding in chunks fails as decoding at once does: at every split of the input into two feeds,
//! and in every chunk size, a type that claims chunked input gives `from_slice`'s value or the
//! kind and field of its error.

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::{Decode, Decoder, ErrorKind, Fed};
use proptest::prelude::*;

type Outcome<T> = Result<T, (ErrorKind, u16)>;

#[derive(Decode, Clone, Debug, Default, PartialEq)]
struct Reading {
    id: u32,
    #[cbor(bytes)]
    key: [u8; 2],
    flag: Option<bool>,
}

/// A map whose unknown entries are counted.
#[derive(Decode, Clone, Debug, Default, PartialEq)]
struct Counted {
    id: u8,
    #[cbor(other)]
    unknown: cbor_static::Skipped,
}

#[derive(Decode, Clone, Debug, Default, PartialEq)]
enum State {
    #[default]
    Idle,
    Busy {
        since: u16,
    },
}

/// One item of each major type and form, from RFC 8949 Appendix A where it has one.
const ITEMS: [&[u8]; 22] = [
    b"\x00",
    b"\x18\x64",
    b"\x1b\x00\x00\x00\x01\x00\x00\x00\x00",
    b"\x20",
    b"\x39\x03\xe7",
    b"\x40",
    b"\x44\x01\x02\x03\x04",
    b"\x5f\x42\x01\x02\x43\x03\x04\x05\xff",
    b"\x60",
    b"\x65hello",
    b"\x7f\x65strea\x64ming\xff",
    b"\x82\x01\x02",
    b"\x9f\x01\x82\x02\x03\xff",
    b"\xa2\x01\x02\x03\x04",
    b"\xa1\x61a\x61b",
    b"\xc1\x1a\x51\x4b\x67\xb0",
    b"\xc2\x49\x01\x00\x00\x00\x00\x00\x00\x00\x00",
    b"\xf4",
    b"\xf6",
    b"\xf9\x3c\x00",
    b"\xfa\x47\xc3\x50\x00",
    b"\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a",
];

/// `bytes` decoded at once, with its error's kind and field.
fn whole<T: for<'de> Decode<'de>>(bytes: &[u8]) -> Outcome<T> {
    cbor_static::from_slice(bytes).map_err(|e| (e.kind, e.field))
}

/// `bytes` fed in `pieces`, the value taken out of its slot.
fn fed<'a, T: for<'de> Decode<'de> + Default>(
    bytes: &[u8],
    pieces: impl IntoIterator<Item = &'a [u8]>,
) -> Outcome<T> {
    fed_by(bytes, pieces, |kind, field, _| (kind, field))
}

/// [`fed`], its error as `refusal` makes it of a kind, field and offset.
fn fed_by<'a, T: for<'de> Decode<'de> + Default, E>(
    bytes: &[u8],
    pieces: impl IntoIterator<Item = &'a [u8]>,
    refusal: impl Fn(ErrorKind, u16, u32) -> E,
) -> Result<T, E> {
    let mut slot = MaybeUninit::uninit();
    let mut decoder = Decoder::<T>::new(&mut slot);
    let mut used = 0;
    for piece in pieces {
        match decoder
            .feed(piece)
            .map_err(|e| refusal(e.kind, e.field, e.offset))?
        {
            Fed::Done(n) if used + n == bytes.len() => break,
            Fed::Done(n) => {
                return Err(refusal(
                    ErrorKind::TrailingInput,
                    0,
                    u32::try_from(used + n).unwrap_or(u32::MAX),
                ));
            }
            Fed::NeedInput => used += piece.len(),
        }
    }
    decoder
        .finish()
        .map(core::mem::take)
        .map_err(|e| refusal(e.kind, e.field, e.offset))
}

fn splits_as_whole<T: for<'de> Decode<'de> + Default + PartialEq + Debug>(bytes: &[u8]) {
    splits_as_whole_by(bytes, core::convert::identity::<T>);
}

fn splits_as_whole_by<T: for<'de> Decode<'de> + Default, K: PartialEq + Debug>(
    bytes: &[u8],
    key: fn(T) -> K,
) {
    assert!(!<T as Decode<'_>>::CONTIGUOUS, "claims contiguous input");
    let expected = whole::<T>(bytes).map(key);
    for at in 0..=bytes.len() {
        let (head, tail) = bytes.split_at(at);
        assert_eq!(
            fed::<T>(bytes, [head, tail]).map(key),
            expected,
            "{name} {bytes:x?} split at {at}",
            name = core::any::type_name::<T>()
        );
    }
    for chunk in 1..=bytes.len() {
        assert_eq!(
            fed::<T>(bytes, bytes.chunks(chunk)).map(key),
            expected,
            "{name} {bytes:x?} in chunks of {chunk}",
            name = core::any::type_name::<T>()
        );
    }
}

fn every_type(bytes: &[u8]) {
    splits_as_whole::<u8>(bytes);
    splits_as_whole::<u32>(bytes);
    splits_as_whole::<i64>(bytes);
    splits_as_whole::<u128>(bytes);
    splits_as_whole::<i128>(bytes);
    splits_as_whole::<bool>(bytes);
    splits_as_whole::<()>(bytes);
    splits_as_whole_by(bytes, f32::to_bits);
    splits_as_whole_by(bytes, f64::to_bits);
    splits_as_whole::<char>(bytes);
    splits_as_whole::<Option<u16>>(bytes);
    splits_as_whole::<[u8; 2]>(bytes);
    splits_as_whole::<(u8, bool)>(bytes);
    splits_as_whole::<Reading>(bytes);
    splits_as_whole::<State>(bytes);
    splits_as_whole::<Counted>(bytes);
    #[cfg(feature = "heapless")]
    {
        splits_as_whole::<heapless::String<8>>(bytes);
        splits_as_whole::<heapless::Vec<u8, 4>>(bytes);
    }
    #[cfg(feature = "alloc")]
    {
        splits_as_whole::<String>(bytes);
        splits_as_whole::<Vec<u16>>(bytes);
        splits_as_whole::<Option<Box<u32>>>(bytes);
    }
}

#[test]
fn every_item_splits_as_it_decodes_whole_into_every_type() {
    for item in ITEMS {
        every_type(item);
    }
}

/// An unknown entry's error names the `other` field whether one chunk holds the entry or not.
#[test]
fn an_unknown_entrys_malformed_value_names_the_other_field_in_any_chunks() {
    let malformed = b"\xa2\x62id\x01\x61z\x62\xff\xfe";
    assert_eq!(
        whole::<Counted>(malformed),
        Err((ErrorKind::InvalidUtf8, 1))
    );
    splits_as_whole::<Counted>(malformed);
}

/// #84: a counted unknown entry whose byte string key is longer than 16 bits can address is
/// skipped past, in any chunks, so this map, cut short, ends early under any length limit.
#[test]
fn a_counted_entry_past_the_16_bit_limit_ends_early_in_any_chunks() {
    let cut = b"\xb6\x5a\x7e\x40\x15\xdd\xac\x11\x34\xc7\x2d\xb4\xe2\x59\x55\xe5\x42\xa9\x69\xa0";
    assert_eq!(whole::<Counted>(cut), Err((ErrorKind::UnexpectedEnd, 0)));
    splits_as_whole::<Counted>(cut);
}

/// A unit variant named by text, as a COSE structure's context is.
#[derive(Decode, Debug, PartialEq)]
enum Word {
    Ab,
}

/// Invalid text is refused at the end of its payload however the input is split: before the fix,
/// a split just past the bad byte reported the end of the first chunk instead.
fn refused_at_the_same_offset_at_every_split<
    T: for<'de> Decode<'de> + Default + Debug + PartialEq,
>(
    bytes: &[u8],
    expected: (ErrorKind, u32),
) {
    assert_eq!(
        cbor_static::from_slice::<T>(bytes).map_err(|e| (e.kind, e.offset)),
        Err(expected),
        "whole {bytes:x?}"
    );
    for at in 0..=bytes.len() {
        let (head, tail) = bytes.split_at(at);
        assert_eq!(
            fed_by::<T, _>(bytes, [head, tail], |kind, _, offset| (kind, offset)),
            Err(expected),
            "{name} {bytes:x?} split at {at}",
            name = core::any::type_name::<T>()
        );
    }
}

/// The fuzzers' first finding: a text-named variant (`a\xffb`).
#[test]
fn invalid_text_naming_a_variant_is_refused_at_its_end_in_any_chunks() {
    refused_at_the_same_offset_at_every_split::<Option<Word>>(
        b"\x63a\xffb",
        (ErrorKind::InvalidUtf8, 4),
    );
}

#[test]
fn an_invalid_text_key_is_refused_at_its_end_in_any_chunks() {
    refused_at_the_same_offset_at_every_split::<Reading>(
        b"\xa1\x63a\xffb\x00",
        (ErrorKind::InvalidUtf8, 5),
    );
}

#[test]
fn an_unknown_invalid_text_key_is_refused_at_its_end_in_any_chunks() {
    refused_at_the_same_offset_at_every_split::<Counted>(
        b"\xa2\x62id\x00\x63a\xffb\x00",
        (ErrorKind::InvalidUtf8, 9),
    );
}

#[test]
fn an_unknown_entrys_invalid_text_is_refused_at_its_end_in_any_chunks() {
    refused_at_the_same_offset_at_every_split::<Reading>(
        b"\xa2\x62id\x00\x61z\x63a\xffb",
        (ErrorKind::InvalidUtf8, 11),
    );
    refused_at_the_same_offset_at_every_split::<Reading>(
        b"\xa2\x62id\x00\x61z\x7f\x63a\xffb\xff",
        (ErrorKind::InvalidUtf8, 12),
    );
}

#[cfg(feature = "alloc")]
#[test]
fn an_owned_invalid_text_is_refused_at_its_end_in_any_chunks() {
    refused_at_the_same_offset_at_every_split::<String>(b"\x63a\xffb", (ErrorKind::InvalidUtf8, 4));
    refused_at_the_same_offset_at_every_split::<String>(
        b"\x7f\x63a\xffb\xff",
        (ErrorKind::InvalidUtf8, 5),
    );
}

/// Text cut short is cut short, whether or not the bytes so far are valid.
#[cfg(feature = "alloc")]
#[test]
fn invalid_text_cut_short_ends_early_in_any_chunks() {
    refused_at_the_same_offset_at_every_split::<String>(
        b"\x63a\xff",
        (ErrorKind::UnexpectedEnd, 3),
    );
}

#[test]
fn a_nan_splits_as_it_decodes_whole_into_every_type() {
    for nan in [
        b"\xf9\x7c\x01".as_slice(),
        b"\xf9\x7f\xb5",
        b"\xfa\x7f\x80\x00\x01",
        b"\xfb\x7f\xf0\x00\x00\x00\x00\x00\x01",
    ] {
        every_type(nan);
    }
}

#[test]
fn a_wrong_type_string_split_across_feeds_is_a_type_mismatch() {
    assert_eq!(
        fed::<u32>(b"\x65hello", [b"\x65he".as_slice(), b"llo"]),
        Err((ErrorKind::TypeMismatch, 0))
    );
}

/// A leaf that takes no string is handed none of its payload, so it must reject the head: one that
/// accepted it would leave the payload to be read as the items after it (`TrailingInput` here).
/// Every leaf marked `Payload::Refused` that a decode reaches through a type: each scalar and its
/// `Option`, the bignums, `Tagged`, a unit variant's null payload, and a `skip` field.
#[test]
fn every_leaf_that_takes_no_string_rejects_one() {
    fn rejects<T: for<'de> Decode<'de> + Debug>(bytes: &[u8]) {
        assert!(
            matches!(
                cbor_static::from_slice::<T>(bytes).map_err(|e| e.kind),
                Err(ErrorKind::TypeMismatch | ErrorKind::UnexpectedTag)
            ),
            "{name} {bytes:x?}",
            name = core::any::type_name::<T>()
        );
    }

    macro_rules! scalars {
        ($bytes:expr; $($ty:ty),+) => {$(
            rejects::<$ty>($bytes);
            rejects::<Option<$ty>>($bytes);
        )+};
    }

    #[derive(Decode, Debug)]
    enum Unit {
        Idle,
    }

    #[derive(Decode, Debug)]
    #[cbor(array)]
    struct Skipping {
        a: u8,
        #[serde(skip)]
        b: u8,
    }

    for string in [
        b"\x65hello".as_slice(),
        b"\x44\x01\x02\x03\x04",
        b"\x60",
        b"\x40",
    ] {
        scalars!(string; u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, (), bool, f32, f64);
        scalars!(string; cbor_static::F16);
        #[cfg(feature = "nightly")]
        scalars!(string; f16);
        rejects::<u128>(string);
        rejects::<i128>(string);
        rejects::<cbor_static::Tagged<1, u8>>(string);
        rejects::<Unit>(&[b"\xa1\x64Idle".as_slice(), string].concat());
        rejects::<Skipping>(&[b"\x82\x01".as_slice(), string].concat());
    }
}

#[test]
fn a_wrong_type_item_is_reported_just_past_its_head_in_any_chunks() {
    for bytes in [
        b"\x65hello".as_slice(),
        b"\x44\x01\x02\x03\x04",
        b"\xf5",
        b"\x80",
        b"\xa0",
    ] {
        assert_eq!(
            cbor_static::from_slice::<u32>(bytes).map_err(|e| (e.kind, e.offset)),
            Err((ErrorKind::TypeMismatch, 1)),
            "{bytes:x?}"
        );
        for at in 0..=bytes.len() {
            let (head, tail) = bytes.split_at(at);
            let mut slot = MaybeUninit::uninit();
            let mut decoder = Decoder::<u32>::new(&mut slot);
            assert_eq!(
                decoder
                    .feed(head)
                    .and_then(|_| decoder.feed(tail))
                    .map_err(|e| (e.kind, e.offset)),
                Err((ErrorKind::TypeMismatch, 1)),
                "{bytes:x?} split at {at}"
            );
        }
    }
}

#[test]
fn a_string_too_long_and_invalid_fails_on_its_length_in_any_chunks() {
    let bytes = b"\x65\x00\x00\x80\x00\x00";
    assert_eq!(whole::<char>(bytes), Err((ErrorKind::TypeMismatch, 0)));
    splits_as_whole::<char>(bytes);
    #[cfg(feature = "heapless")]
    {
        assert_eq!(
            whole::<heapless::String<4>>(bytes),
            Err((ErrorKind::OutOfRange, 0))
        );
        splits_as_whole::<heapless::String<4>>(bytes);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: if cfg!(miri) { 4 } else { 512 },
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_bytes_split_as_they_decode_whole(bytes in proptest::collection::vec(any::<u8>(), 0..24)) {
        every_type(&bytes);
    }
}
