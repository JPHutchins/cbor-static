//! Tags (#12 Q25): `Tagged<TAG, T>` over any `T`, the date/time tags 0 and 1, and the self-describe
//! tag 55799, which decoding takes or leaves (RFC 8949 §3.4).

mod common;

use core::mem::MaybeUninit;

use cbor_static::{Cbor, DateTime, Epoch, ErrorKind, SelfDescribed, Tagged};
use common::{chunked_decode, decode, decode_strict, encode, golden};

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Point {
    x: u32,
    y: u32,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
enum Shape {
    Dot,
    Circle(u32),
}

#[test]
fn a_tag_wraps_any_value() {
    golden(
        &Tagged::<1000, Point>(Point { x: 1, y: 2 }),
        b"\xd9\x03\xe8\xa2\x61x\x01\x61y\x02",
    );
    golden(&Tagged::<5, (u32, bool)>((1, true)), b"\xc5\x82\x01\xf5");
    golden(
        &Tagged::<6, Shape>(Shape::Circle(3)),
        b"\xc6\xa1\x66Circle\x03",
    );
    golden(&Tagged::<6, Shape>(Shape::Dot), b"\xc6\x63Dot");
    golden(&Tagged::<1, Tagged<2, u64>>(Tagged(7)), b"\xc1\xc2\x07");
    golden(&[Some(Tagged::<7, u32>(1)), None], b"\x82\xc7\x01\xf6");
}

#[test]
fn only_the_tag_itself_is_accepted() {
    assert_eq!(
        decode::<Tagged<1000, Point>>(b"\xa2\x61x\x01\x61y\x02"),
        Err(ErrorKind::UnexpectedTag)
    );
    assert_eq!(
        decode::<Tagged<1000, Point>>(b"\xd9\x03\xe9\xa2\x61x\x01\x61y\x02"),
        Err(ErrorKind::UnexpectedTag)
    );
    assert_eq!(
        decode::<Tagged<1, u32>>(b"\xc1\xc1\x01"),
        Err(ErrorKind::TypeMismatch)
    );
}

#[test]
fn the_date_time_tags_take_rfc_8949_appendix_a() {
    let mut slot = MaybeUninit::uninit();
    let at: &mut DateTime =
        cbor_static::decode_into(b"\xc0\x742013-03-21T20:04:00Z", &mut slot).expect("decodes");
    assert_eq!(at.0, "2013-03-21T20:04:00Z");
    let epoch: Epoch<u64> = Tagged(1_363_896_240);
    golden(&epoch, b"\xc1\x1a\x51\x4b\x67\xb0");
    assert_eq!(
        decode::<Epoch<f64>>(b"\xc1\xfb\x41\xd4\x52\xd9\xec\x20\x00\x00").map(|e| e.0.to_bits()),
        Ok(0x41d4_52d9_ec20_0000)
    );
}

#[test]
fn a_self_described_value_is_written_with_the_tag_and_read_with_or_without_it() {
    golden(
        &SelfDescribed(Point { x: 1, y: 2 }),
        b"\xd9\xd9\xf7\xa2\x61x\x01\x61y\x02",
    );
    assert_eq!(
        decode::<SelfDescribed<Point>>(b"\xa2\x61x\x01\x61y\x02"),
        Ok(SelfDescribed(Point { x: 1, y: 2 }))
    );
    assert_eq!(
        decode::<SelfDescribed<u32>>(b"\xc1\x07"),
        Err(ErrorKind::TypeMismatch)
    );
}

/// A tag from 256 to 65535 starts with the same byte as 55799, so the value's own head is read
/// whole and given back: in place, or from the carry when a chunk boundary split it.
#[test]
fn a_self_described_tagged_value_is_told_apart_from_its_own_tag_at_every_split() {
    type Thousand = SelfDescribed<Tagged<1000, u32>>;
    let with = b"\xd9\xd9\xf7\xd9\x03\xe8\x01";
    let without = b"\xd9\x03\xe8\x01";
    assert_eq!(encode(&SelfDescribed(Tagged::<1000, u32>(1))), with);
    for bytes in [with.as_slice(), without] {
        for chunk in 1..=bytes.len() {
            assert_eq!(
                chunked_decode::<Thousand>(bytes, chunk),
                Ok(SelfDescribed(Tagged(1))),
                "{bytes:x?} in chunks of {chunk}"
            );
        }
    }
}

#[test]
fn self_description_nests() {
    type Twice = SelfDescribed<SelfDescribed<u32>>;
    golden(
        &SelfDescribed(SelfDescribed(7u32)),
        b"\xd9\xd9\xf7\xd9\xd9\xf7\x07",
    );
    for bytes in [b"\xd9\xd9\xf7\x07".as_slice(), b"\x07"] {
        for chunk in 1..=bytes.len() {
            assert_eq!(
                chunked_decode::<Twice>(bytes, chunk),
                Ok(SelfDescribed(SelfDescribed(7))),
                "{bytes:x?} in chunks of {chunk}"
            );
        }
    }
}

#[test]
fn strict_takes_only_the_preferred_self_describe_tag() {
    assert_eq!(
        decode_strict::<SelfDescribed<u32>>(b"\xd9\xd9\xf7\x07"),
        Ok(SelfDescribed(7))
    );
    assert_eq!(
        decode_strict::<SelfDescribed<u32>>(b"\xda\x00\x00\xd9\xf7\x07"),
        Err(ErrorKind::NotDeterministic)
    );
    assert_eq!(
        decode::<SelfDescribed<u32>>(b"\xda\x00\x00\xd9\xf7\x07"),
        Ok(SelfDescribed(7))
    );
}

std::thread_local! {
    static DROPPED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

#[derive(Cbor, Debug)]
struct Counted(u32);

impl Drop for Counted {
    fn drop(&mut self) {
        DROPPED.set(DROPPED.get() + 1);
    }
}

#[derive(Cbor, Debug)]
struct Holder {
    tagged: Tagged<7, Counted>,
    described: SelfDescribed<Counted>,
    after: u32,
}

#[test]
fn a_decoded_tagged_value_is_dropped_when_a_later_field_fails() {
    DROPPED.set(0);
    let mut slot = MaybeUninit::<Holder>::uninit();
    assert_eq!(
        cbor_static::decode_into(
            b"\xa3\x66tagged\xc7\x01\x69described\x02\x65after\xf6",
            &mut slot
        )
        .map(|_| ())
        .map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(DROPPED.get(), 2);
}
