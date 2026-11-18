#![forbid(unsafe_code)]
//! R-204: encoding and decoding resume across chunks. Any message is the same in chunks of 1 to 64
//! bytes as in one, and a head of up to 9 bytes split between two chunks is carried across them,
//! in each direction, at every point the split can fall.

mod common;

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::{Cbor, Decode, Decoder, Encode, Encoder, Fed, Progress, Tagged};
use proptest::prelude::*;

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Message {
    id: u64,
    offset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reading: Option<Tagged<1000, u32>>,
    samples: [Option<i32>; 3],
    #[cbor(bytes)]
    key: [u8; 4],
    state: State,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
enum State {
    Idle,
    Busy { since: u32 },
    Fault(i16),
}

fn state() -> impl Strategy<Value = State> {
    prop_oneof![
        Just(State::Idle),
        any::<u32>().prop_map(|since| State::Busy { since }),
        any::<i16>().prop_map(State::Fault),
    ]
}

fn message() -> impl Strategy<Value = Message> {
    (
        any::<u64>(),
        any::<i64>(),
        any::<Option<u32>>(),
        any::<[Option<i32>; 3]>(),
        any::<[u8; 4]>(),
        state(),
    )
        .prop_map(|(id, offset, reading, samples, key, state)| Message {
            id,
            offset,
            reading: reading.map(Tagged),
            samples,
            key,
            state,
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: if cfg!(miri) { 4 } else { 256 },
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_message_is_the_same_in_every_chunk_size(message in message()) {
        common::golden(&message, &common::encode(&message));
    }
}

fn decoded_across_every_split<T>(value: &T, bytes: &[u8])
where
    T: for<'de> Decode<'de> + PartialEq + Debug,
{
    for split in 1..bytes.len() {
        let (first, second) = bytes.split_at(split);
        let mut slot = MaybeUninit::uninit();
        let mut decoder = Decoder::<T>::new(&mut slot);
        assert_eq!(decoder.feed(first), Ok(Fed::NeedInput), "split at {split}");
        assert_eq!(
            decoder.feed(second),
            Ok(Fed::Done(second.len())),
            "split at {split}"
        );
        assert_eq!(decoder.finish().map(|v| &*v == value), Ok(true));
    }
}

fn encoded_across_every_split<T: Encode>(value: &T, bytes: &[u8]) {
    for split in 1..bytes.len() {
        let mut out = vec![0u8; bytes.len()];
        let (first, second) = out.split_at_mut(split);
        let mut encoder = Encoder::new(value);
        assert_eq!(
            encoder.resume(first),
            Ok(Progress::NeedSpace(split)),
            "split at {split}"
        );
        assert_eq!(
            encoder.resume(second),
            Ok(Progress::Done(bytes.len() - split)),
            "split at {split}"
        );
        assert_eq!(out, bytes, "split at {split}");
    }
}

fn across_every_split<T>(value: &T, bytes: &[u8])
where
    T: Encode + for<'de> Decode<'de> + PartialEq + Debug,
{
    encoded_across_every_split(value, bytes);
    decoded_across_every_split(value, bytes);
}

#[test]
fn integer_heads() {
    across_every_split(&u64::MAX, b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff");
    across_every_split(&i64::MIN, b"\x3b\x7f\xff\xff\xff\xff\xff\xff\xff");
}

#[test]
fn float_head() {
    across_every_split(&1.1f64, b"\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a");
}

#[test]
fn tag_head_and_its_item() {
    across_every_split(
        &Tagged::<{ u64::MAX }, u8>(7),
        b"\xdb\xff\xff\xff\xff\xff\xff\xff\xff\x07",
    );
}

#[test]
fn length_heads_and_their_items() {
    decoded_across_every_split(&[1u8], b"\x9b\x00\x00\x00\x00\x00\x00\x00\x01\x01");
    decoded_across_every_split(
        &[(5u8, true)],
        b"\x9b\x00\x00\x00\x00\x00\x00\x00\x01\x82\x05\xf5",
    );
}

#[cfg(feature = "heapless")]
#[test]
fn string_and_map_length_heads() {
    use heapless::{LinearMap, String};

    decoded_across_every_split(
        &String::<4>::try_from("ab").expect("fits"),
        b"\x7b\x00\x00\x00\x00\x00\x00\x00\x02ab",
    );
    decoded_across_every_split(
        &[(1u8, 2u8)].into_iter().collect::<LinearMap<u8, u8, 2>>(),
        b"\xbb\x00\x00\x00\x00\x00\x00\x00\x01\x01\x02",
    );
}
