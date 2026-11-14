#![forbid(unsafe_code)]

mod common;

use core::fmt::Debug;

use cbor_static::{Decode, Encode, ErrorKind};
use common::{chunked_decode, chunked_decode_strict, decode, decode_strict, encode};

#[test]
fn integer_fields_reject_floats_and_float_fields_reject_integers() {
    for float in [
        b"\xf9\x3c\x00".as_slice(),
        b"\xfa\x3f\x80\x00\x00",
        b"\xfb\x3f\xf0\0\0\0\0\0\0",
    ] {
        assert_eq!(
            decode::<u32>(float),
            Err(ErrorKind::TypeMismatch),
            "{float:x?}"
        );
        assert_eq!(
            decode::<u64>(float),
            Err(ErrorKind::TypeMismatch),
            "{float:x?}"
        );
        assert_eq!(
            decode::<i32>(float),
            Err(ErrorKind::TypeMismatch),
            "{float:x?}"
        );
        assert_eq!(
            decode::<u128>(float),
            Err(ErrorKind::TypeMismatch),
            "{float:x?}"
        );
        assert_eq!(
            decode::<i128>(float),
            Err(ErrorKind::TypeMismatch),
            "{float:x?}"
        );
    }
    for integer in [b"\x01".as_slice(), b"\x20", b"\xc2\x41\x01"] {
        assert_eq!(
            decode::<f64>(integer),
            Err(ErrorKind::TypeMismatch),
            "{integer:x?}"
        );
        assert_eq!(
            decode::<f32>(integer),
            Err(ErrorKind::TypeMismatch),
            "{integer:x?}"
        );
    }
}

#[test]
fn a_float_narrows_only_when_exact() {
    assert_eq!(decode::<f32>(b"\xfb\x3f\xf8\0\0\0\0\0\0"), Ok(1.5));
    assert_eq!(
        decode::<f32>(b"\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        decode::<f32>(b"\xfb\x47\xef\xff\xff\xe0\0\0\0").map(f32::to_bits),
        Ok(f32::MAX.to_bits())
    );
    assert_eq!(
        decode::<f32>(b"\xfb\x47\xf0\0\0\0\0\0\0"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        decode::<f32>(b"\xfb\x36\xa0\0\0\0\0\0\0").map(f32::to_bits),
        Ok(1)
    );
    assert_eq!(
        decode::<f32>(b"\xfb\x36\x90\0\0\0\0\0\0"),
        Err(ErrorKind::OutOfRange)
    );
}

#[test]
fn a_nan_narrows_only_when_its_payload_fits() {
    assert_eq!(
        decode::<f32>(b"\xfb\x7f\xf8\0\0\x20\0\0\0").map(f32::to_bits),
        Ok(0x7fc0_0001)
    );
    assert_eq!(
        decode::<f32>(b"\xfb\x7f\xf8\0\0\0\0\0\x01"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        decode::<f32>(b"\xfb\xff\xf0\0\0\x10\0\0\0"),
        Err(ErrorKind::OutOfRange)
    );
}

#[test]
fn an_integer_narrows_only_when_in_range() {
    assert_eq!(decode::<u32>(b"\x1a\xff\xff\xff\xff"), Ok(u32::MAX));
    assert_eq!(
        decode::<u32>(b"\x1b\0\0\0\x01\0\0\0\0"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(decode::<u32>(b"\x20"), Err(ErrorKind::OutOfRange));
    assert_eq!(decode::<u64>(b"\x20"), Err(ErrorKind::OutOfRange));
    assert_eq!(decode::<i32>(b"\x1a\x7f\xff\xff\xff"), Ok(i32::MAX));
    assert_eq!(decode::<i32>(b"\x1a\x80\0\0\0"), Err(ErrorKind::OutOfRange));
    assert_eq!(decode::<i32>(b"\x3a\x7f\xff\xff\xff"), Ok(i32::MIN));
    assert_eq!(decode::<i32>(b"\x3a\x80\0\0\0"), Err(ErrorKind::OutOfRange));
    assert_eq!(decode::<u128>(b"\x20"), Err(ErrorKind::OutOfRange));
    assert_eq!(
        decode::<u128>(b"\x3b\xff\xff\xff\xff\xff\xff\xff\xff"),
        Err(ErrorKind::OutOfRange)
    );
}

/// Each width takes its own extremes, encoded as the same integer any wider width writes, and
/// rejects the integers just past them that a plain head can carry.
fn narrows_at<T>(min: i128, max: i128)
where
    T: for<'de> Decode<'de> + Encode + Copy + PartialEq + Debug + TryFrom<i128>,
{
    for edge in [min, max] {
        let value = T::try_from(edge).ok();
        assert!(value.is_some(), "{edge}");
        assert_eq!(decode::<T>(&encode(&edge)).ok(), value, "{edge}");
        assert_eq!(value.map(|v| encode(&v)), Some(encode(&edge)), "{edge}");
    }
    for past in [min - 1, max + 1]
        .into_iter()
        .filter(|past| (-(1i128 << 64)..(1i128 << 64)).contains(past))
    {
        assert_eq!(
            decode::<T>(&encode(&past)),
            Err(ErrorKind::OutOfRange),
            "{past}"
        );
    }
}

#[test]
fn every_integer_width_narrows_only_when_in_range() {
    narrows_at::<u8>(0, u8::MAX.into());
    narrows_at::<u16>(0, u16::MAX.into());
    narrows_at::<u32>(0, u32::MAX.into());
    narrows_at::<u64>(0, u64::MAX.into());
    narrows_at::<usize>(0, usize::MAX.try_into().expect("usize fits i128"));
    narrows_at::<i8>(i8::MIN.into(), i8::MAX.into());
    narrows_at::<i16>(i16::MIN.into(), i16::MAX.into());
    narrows_at::<i32>(i32::MIN.into(), i32::MAX.into());
    narrows_at::<i64>(i64::MIN.into(), i64::MAX.into());
    narrows_at::<isize>(
        isize::MIN.try_into().expect("isize fits i128"),
        isize::MAX.try_into().expect("isize fits i128"),
    );
}

#[test]
fn only_128_bit_fields_take_bignums() {
    for bignum in [b"\xc2\x41\x01".as_slice(), b"\xc3\x40"] {
        assert_eq!(
            decode::<u32>(bignum),
            Err(ErrorKind::TypeMismatch),
            "{bignum:x?}"
        );
        assert_eq!(
            decode::<u64>(bignum),
            Err(ErrorKind::TypeMismatch),
            "{bignum:x?}"
        );
        assert_eq!(
            decode::<i32>(bignum),
            Err(ErrorKind::TypeMismatch),
            "{bignum:x?}"
        );
    }
    assert_eq!(decode::<u128>(b"\xc2\x41\x01"), Ok(1));
    assert_eq!(decode::<i128>(b"\xc3\x41\x01"), Ok(-2));
    assert_eq!(decode::<u128>(b"\xc3\x40"), Err(ErrorKind::OutOfRange));
}

#[test]
fn bignums_take_their_whole_range_and_no_more() {
    let max: [&[u8]; 4] = [
        b"\xc2\x50\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
        b"\xc2\x50\x7f\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
        b"\xc3\x50\x7f\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
        b"\xc2\x51\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    ];
    assert_eq!(decode::<u128>(max[0]), Ok(u128::MAX));
    assert_eq!(decode::<i128>(max[0]), Err(ErrorKind::OutOfRange));
    assert_eq!(decode::<i128>(max[1]), Ok(i128::MAX));
    assert_eq!(decode::<i128>(max[2]), Ok(i128::MIN));
    assert_eq!(decode::<u128>(max[3]), Err(ErrorKind::OutOfRange));
    assert_eq!(
        decode::<i128>(b"\xc3\x50\x80\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"),
        Err(ErrorKind::OutOfRange)
    );
}

#[test]
fn leading_zeros_and_small_bignums_are_accepted_but_not_strictly() {
    for (bytes, value) in [
        (b"\xc2\x43\0\0\x01".as_slice(), 1),
        (b"\xc2\x41\x01", 1),
        (
            b"\xc2\x51\0\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
            u128::MAX,
        ),
    ] {
        assert_eq!(decode::<u128>(bytes), Ok(value), "{bytes:x?}");
        assert_eq!(
            decode_strict::<u128>(bytes),
            Err(ErrorKind::NotDeterministic),
            "{bytes:x?}"
        );
    }
}

#[test]
fn an_empty_bignum_is_zero_or_minus_one() {
    assert_eq!(decode::<u128>(b"\xc2\x40"), Ok(0));
    assert_eq!(decode::<i128>(b"\xc2\x40"), Ok(0));
    assert_eq!(decode::<i128>(b"\xc3\x40"), Ok(-1));
}

/// #20: a `u128` or `i128` bignum's digits are copied into it as they arrive, so they decode in
/// any chunks, definite or indefinite; #11 Q2's contiguous-only digits are superseded for these
/// owned targets.
#[test]
fn bignum_digits_decode_in_any_chunks_definite_or_indefinite() {
    let indefinite = b"\xc2\x5f\x41\x01\x40\x48\0\0\0\0\0\0\0\0\xff";
    for chunk in 1..=indefinite.len() {
        assert_eq!(
            chunked_decode::<u128>(indefinite, chunk),
            Ok(1 << 64),
            "chunks of {chunk}"
        );
    }
    assert_eq!(decode::<i128>(b"\xc3\x5f\x41\x01\xff"), Ok(-2));
    assert_eq!(decode::<u128>(b"\xc2\x5f\xff"), Ok(0));
    for value in [1 << 64, (1 << 120) + 5, u128::MAX] {
        every_chunk(value);
    }
    for value in [i128::MIN, -(1 << 64) - 1, 1 << 64, i128::MAX] {
        every_chunk(value);
    }
    assert_eq!(
        decode::<u128>(b"\xc2\x5f\x61a\xff"),
        Err(ErrorKind::NotWellFormed)
    );
}

/// `value` encodes as a bignum and decodes back in every chunk size.
fn every_chunk<T>(value: T)
where
    T: for<'de> Decode<'de> + Encode + Copy + PartialEq + Debug,
{
    let bytes = encode(&value);
    assert_eq!(bytes.first().map(|tag| tag & 0xfe), Some(0xc2), "{value:?}");
    for chunk in 1..=bytes.len() {
        assert_eq!(
            chunked_decode::<T>(&bytes, chunk),
            Ok(value),
            "{value:?} in chunks of {chunk}"
        );
    }
}

#[test]
fn seventeen_significant_digits_are_out_of_range_in_any_chunks() {
    let past = b"\xc2\x51\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    for chunk in 1..=past.len() {
        assert_eq!(
            chunked_decode::<u128>(past, chunk),
            Err(ErrorKind::OutOfRange),
            "chunks of {chunk}"
        );
        assert_eq!(
            chunked_decode::<i128>(past, chunk),
            Err(ErrorKind::OutOfRange),
            "chunks of {chunk}"
        );
    }
}

/// `Strict` takes a bignum of more digits than a `u64` holds, the first not zero, definite, in any
/// chunks, and nothing else.
#[test]
fn strict_bignums_are_checked_in_any_chunks() {
    let cases: [(&[u8], Result<u128, ErrorKind>); 5] = [
        (b"\xc2\x49\x01\0\0\0\0\0\0\0\0", Ok(1 << 64)),
        (
            b"\xc2\x4a\0\x01\0\0\0\0\0\0\0\0",
            Err(ErrorKind::NotDeterministic),
        ),
        (
            b"\xc2\x49\0\xff\xff\xff\xff\xff\xff\xff\xff",
            Err(ErrorKind::NotDeterministic),
        ),
        (
            b"\xc2\x48\xff\xff\xff\xff\xff\xff\xff\xff",
            Err(ErrorKind::NotDeterministic),
        ),
        (
            b"\xc2\x5f\x49\x01\0\0\0\0\0\0\0\0\xff",
            Err(ErrorKind::NotDeterministic),
        ),
    ];
    for (bytes, expected) in cases {
        for chunk in 1..=bytes.len() {
            assert_eq!(
                chunked_decode_strict::<u128>(bytes, chunk),
                expected,
                "{bytes:x?} in chunks of {chunk}"
            );
        }
    }
}

#[test]
fn negative_zero_keeps_its_sign() {
    for bytes in [
        b"\xf9\x80\x00".as_slice(),
        b"\xfa\x80\0\0\0",
        b"\xfb\x80\0\0\0\0\0\0\0",
    ] {
        assert_eq!(
            decode::<f64>(bytes).map(f64::to_bits),
            Ok(0x8000_0000_0000_0000),
            "{bytes:x?}"
        );
        assert_eq!(
            decode::<f32>(bytes).map(f32::to_bits),
            Ok(0x8000_0000),
            "{bytes:x?}"
        );
    }
    assert_eq!(encode(&-0.0f64), b"\xf9\x80\x00");
}

#[test]
fn nan_payloads_round_trip_through_f64() {
    for (bytes, bits) in [
        (b"\xf9\x7e\x01".as_slice(), 0x7ff8_0400_0000_0000_u64),
        (b"\xf9\xfc\x01", 0xfff0_0400_0000_0000),
        (b"\xfa\x7f\x80\x00\x01", 0x7ff0_0000_2000_0000),
        (b"\xfb\x7f\xf0\0\0\0\0\0\x01", 0x7ff0_0000_0000_0001),
    ] {
        assert_eq!(
            decode::<f64>(bytes).map(f64::to_bits),
            Ok(bits),
            "{bytes:x?}"
        );
        assert_eq!(encode(&f64::from_bits(bits)), bytes, "{bytes:x?}");
    }
}
