#![forbid(unsafe_code)]

mod common;

use cbor_static::ErrorKind;
use common::{decode, decode_strict, encode};

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
