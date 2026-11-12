#![forbid(unsafe_code)]

mod common;

use core::fmt::Debug;

use cbor_static::{Cbor, Decode, ErrorKind, Tagged};
use common::{chunked_decode, chunked_decode_strict, decode, decode_strict, encode};

/// Under `Strict`, `bytes` decodes to `expected`, and by default to `lenient`: whole, and at every
/// chunk size, where a borrowed payload split across chunks is `NeedContiguous` instead.
fn strictly<'de, T: Decode<'de> + Copy + PartialEq + Debug>(
    bytes: &'de [u8],
    expected: Result<T, ErrorKind>,
    lenient: Result<T, ErrorKind>,
) {
    assert_eq!(
        (decode_strict::<T>(bytes), decode::<T>(bytes)),
        (expected, lenient),
        "{bytes:x?}"
    );
    for chunk in 1..bytes.len() {
        for (outcome, wanted) in [
            (chunked_decode_strict::<T>(bytes, chunk), expected),
            (chunked_decode::<T>(bytes, chunk), lenient),
        ] {
            assert!(
                outcome == wanted || outcome == Err(ErrorKind::NeedContiguous),
                "{outcome:?} in chunks of {chunk}: {bytes:x?}"
            );
        }
    }
}

/// Rejected under `Strict` only.
fn rejected<'de, T: Decode<'de> + Copy + PartialEq + Debug>(bytes: &'de [u8], lenient: T) {
    strictly(bytes, Err(ErrorKind::NotDeterministic), Ok(lenient));
}

/// Accepted under both profiles.
fn accepted<'de, T: Decode<'de> + Copy + PartialEq + Debug>(bytes: &'de [u8], value: T) {
    strictly(bytes, Ok(value), Ok(value));
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct One {
    a: u32,
}

#[test]
fn non_preferred_heads_are_not_deterministic() {
    accepted::<u32>(b"\x17", 23);
    accepted::<u32>(b"\x18\x18", 24);
    accepted::<u32>(b"\x19\x01\x00", 256);
    accepted::<i32>(b"\x38\x18", -25);
    rejected::<u32>(b"\x18\x17", 23);
    rejected::<u32>(b"\x19\x00\xff", 255);
    rejected::<u64>(b"\x1b\0\0\0\0\xff\xff\xff\xff", 0xffff_ffff);
    rejected::<i32>(b"\x38\x00", -1);
    rejected::<One>(b"\xb8\x01\x61a\x01", One { a: 1 });
    rejected::<One>(b"\xa1\x78\x01a\x01", One { a: 1 });
    rejected::<(u32, u32)>(b"\x98\x02\x01\x02", (1, 2));
    rejected::<Tagged<1, u32>>(b"\xd8\x01\x01", Tagged(1));
    rejected::<&str>(b"\x78\x01a", "a");
    rejected::<&[u8]>(b"\x59\x00\x01a", b"a");
}

#[test]
fn non_shortest_floats_are_not_deterministic() {
    accepted::<f64>(b"\xf9\x3c\x00", 1.0);
    accepted::<f64>(b"\xfa\x47\xc3\x50\x00", 100_000.0);
    accepted::<f64>(b"\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a", 1.1);
    accepted::<f32>(b"\xf9\xfc\x00", f32::NEG_INFINITY);
    rejected::<f64>(b"\xfa\x3f\x80\x00\x00", 1.0);
    rejected::<f64>(b"\xfb\x3f\xf0\0\0\0\0\0\0", 1.0);
    rejected::<f64>(b"\xfb\x40\xf8\x6a\0\0\0\0\0", 100_000.0);
    rejected::<f32>(b"\xfa\x80\x00\x00\x00", -0.0);
}

#[test]
fn only_the_canonical_nan_is_deterministic() {
    let bits = |bytes: &[u8]| {
        (
            decode_strict::<f64>(bytes).map(f64::to_bits),
            decode::<f64>(bytes).map(f64::to_bits),
        )
    };
    assert_eq!(
        bits(b"\xf9\x7e\x00"),
        (Ok(0x7ff8_0000_0000_0000), Ok(0x7ff8_0000_0000_0000))
    );
    assert_eq!(
        decode_strict::<f32>(b"\xf9\x7e\x00").map(f32::to_bits),
        Ok(0x7fc0_0000)
    );
    for (bytes, lenient) in [
        (b"\xf9\x7e\x01".as_slice(), 0x7ff8_0400_0000_0000),
        (b"\xf9\xfe\x00", 0xfff8_0000_0000_0000),
        (b"\xf9\x7c\x01", 0x7ff0_0400_0000_0000),
        (b"\xfa\x7f\xc0\x00\x00", 0x7ff8_0000_0000_0000),
        (b"\xfb\x7f\xf8\0\0\0\0\0\0", 0x7ff8_0000_0000_0000),
        (b"\xfb\x7f\xf8\0\0\0\0\0\x01", 0x7ff8_0000_0000_0001),
    ] {
        assert_eq!(
            bits(bytes),
            (Err(ErrorKind::NotDeterministic), Ok(lenient)),
            "{bytes:x?}"
        );
    }
}

#[test]
fn indefinite_lengths_are_not_deterministic() {
    rejected::<One>(b"\xbf\x61a\x01\xff", One { a: 1 });
    rejected::<[u32; 2]>(b"\x9f\x01\x02\xff", [1, 2]);
    rejected::<(u32, u32)>(b"\x9f\x01\x02\xff", (1, 2));
    strictly::<&str>(
        b"\x7f\x61a\xff",
        Err(ErrorKind::NotDeterministic),
        Err(ErrorKind::IndefiniteString),
    );
    strictly::<&[u8]>(
        b"\x5f\x41a\xff",
        Err(ErrorKind::NotDeterministic),
        Err(ErrorKind::IndefiniteString),
    );
}

#[test]
fn non_preferred_bignums_are_not_deterministic() {
    let beyond_u64 = 1u128 << 64;
    accepted::<u128>(b"\xc2\x49\x01\0\0\0\0\0\0\0\0", beyond_u64);
    accepted::<u128>(
        b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff",
        u128::from(u64::MAX),
    );
    accepted::<i128>(b"\xc3\x49\x01\0\0\0\0\0\0\0\0", -1 - (1i128 << 64));
    accepted::<i128>(b"\x3b\xff\xff\xff\xff\xff\xff\xff\xff", -(1i128 << 64));
    rejected::<u128>(b"\xc2\x40", 0);
    rejected::<u128>(b"\xc2\x41\x01", 1);
    rejected::<u128>(
        b"\xc2\x48\xff\xff\xff\xff\xff\xff\xff\xff",
        u128::from(u64::MAX),
    );
    rejected::<u128>(b"\xc2\x4a\x00\x01\0\0\0\0\0\0\0\0", beyond_u64);
    rejected::<u128>(b"\xd8\x02\x49\x01\0\0\0\0\0\0\0\0", beyond_u64);
    rejected::<i128>(b"\xc3\x40", -1);
    rejected::<i128>(b"\xc3\x48\xff\xff\xff\xff\xff\xff\xff\xff", -(1i128 << 64));
    rejected::<i128>(b"\xc2\x4a\x00\x01\0\0\0\0\0\0\0\0", 1i128 << 64);
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Lengths {
    aa: u32,
    b: u32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Labels {
    kid: u32,
    #[cbor(key = -1)]
    crv: u32,
    #[cbor(key = 1)]
    alg: i32,
    #[cbor(key = 24, default, skip_serializing_if = "Option::is_none")]
    big: Option<u32>,
}

#[test]
fn keys_out_of_order_are_not_deterministic() {
    let lengths = Lengths { aa: 2, b: 1 };
    accepted(b"\xa2\x61b\x01\x62aa\x02", lengths);
    rejected(b"\xa2\x62aa\x02\x61b\x01", lengths);
    assert_eq!(
        chunked_decode_strict::<Lengths>(b"\xa2\x62aa\x02\x61b\x01", 1),
        Err(ErrorKind::NotDeterministic),
        "checked against the descriptor, so a byte at a time"
    );
    let labels = Labels {
        kid: 3,
        crv: 1,
        alg: -7,
        big: None,
    };
    accepted(b"\xa3\x01\x26\x20\x01\x63kid\x03", labels);
    accepted(
        b"\xa4\x01\x26\x18\x18\x05\x20\x01\x63kid\x03",
        Labels {
            big: Some(5),
            ..labels
        },
    );
    rejected(b"\xa3\x20\x01\x01\x26\x63kid\x03", labels);
    rejected(b"\xa3\x01\x26\x63kid\x03\x20\x01", labels);
    rejected(
        b"\xa4\x01\x26\x20\x01\x18\x18\x05\x63kid\x03",
        Labels {
            big: Some(5),
            ..labels
        },
    );
    strictly::<Lengths>(
        b"\xa2\x61b\x01\x61b\x02",
        Err(ErrorKind::DuplicateKey),
        Err(ErrorKind::DuplicateKey),
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Unknowing {
    #[cbor(key = 1)]
    a: u32,
    b: u32,
}

#[test]
fn unknown_keys_are_rejected() {
    let value = Unknowing { a: 1, b: 2 };
    for bytes in [
        b"\xa3\x01\x01\x02\x00\x61b\x02".as_slice(),
        b"\xa3\x01\x01\x61a\x00\x61b\x02",
        b"\xa3\x01\x01\x61b\x02\x61c\x00",
        b"\xa3\x01\x01\x61b\x02\x80\x00",
    ] {
        strictly(bytes, Err(ErrorKind::UnknownField), Ok(value));
    }
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "type")]
enum Event {
    Start,
    Move { x: i32 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "t", content = "c")]
enum Message {
    Data(u32),
}

/// Internally and adjacently tagged maps need contiguous input (D-10).
fn whole<'de, T: Decode<'de> + Copy + PartialEq + Debug>(
    bytes: &'de [u8],
    expected: Result<T, ErrorKind>,
    lenient: Result<T, ErrorKind>,
) {
    assert_eq!(
        (decode_strict::<T>(bytes), decode::<T>(bytes)),
        (expected, lenient),
        "{bytes:x?}"
    );
}

#[test]
fn a_tag_key_takes_its_place_in_the_order() {
    let moved = Event::Move { x: 1 };
    whole(b"\xa2\x61x\x01\x64type\x64Move", Ok(moved), Ok(moved));
    whole(b"\xa1\x64type\x65Start", Ok(Event::Start), Ok(Event::Start));
    whole(
        b"\xa2\x64type\x64Move\x61x\x01",
        Err(ErrorKind::NotDeterministic),
        Ok(moved),
    );
    whole(
        b"\xa3\x61x\x01\x61y\x02\x64type\x64Move",
        Err(ErrorKind::UnknownField),
        Ok(moved),
    );
    whole(
        b"\xa2\x61y\x02\x64type\x65Start",
        Err(ErrorKind::UnknownField),
        Ok(Event::Start),
    );
    let data = Message::Data(7);
    whole(b"\xa2\x61c\x07\x61t\x64Data", Ok(data), Ok(data));
    whole(
        b"\xa2\x61t\x64Data\x61c\x07",
        Err(ErrorKind::NotDeterministic),
        Ok(data),
    );
    whole(
        b"\xa3\x61c\x07\x61t\x64Data\x61u\x00",
        Err(ErrorKind::UnknownField),
        Ok(data),
    );
    whole(
        b"\xa3\x61c\x07\x61t\x64Data\x64Data\x00",
        Err(ErrorKind::UnknownField),
        Ok(data),
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Sorted<'a> {
    a: u32,
    b: Option<i32>,
    c: f64,
    d: &'a str,
    e: (bool, f32),
    f: [u128; 2],
}

/// `sorted` with its float compared by bits, so that a NaN equals itself.
fn by_bits(sorted: Result<Sorted<'_>, ErrorKind>) -> Result<(u64, Sorted<'_>), ErrorKind> {
    sorted.map(|sorted| (sorted.c.to_bits(), Sorted { c: 0.0, ..sorted }))
}

#[test]
fn what_the_encoder_writes_in_sorted_order_is_deterministic() {
    for value in [
        Sorted {
            a: 24,
            b: None,
            c: f64::NAN,
            d: "",
            e: (true, -0.0),
            f: [1 << 64, 1],
        },
        Sorted {
            a: u32::MAX,
            b: Some(-24),
            c: 1.1,
            d: "deterministic",
            e: (false, 65504.0),
            f: [u128::MAX, 0],
        },
    ] {
        let bytes = encode(&value);
        assert_eq!(by_bits(decode_strict(&bytes)), by_bits(Ok(value)));
        for chunk in 1..bytes.len() {
            let outcome = by_bits(chunked_decode_strict(&bytes, chunk));
            assert!(
                outcome == by_bits(Ok(value)) || outcome == Err(ErrorKind::NeedContiguous),
                "{outcome:?} in chunks of {chunk}"
            );
        }
    }
}
