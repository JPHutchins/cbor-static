//! R-101 and R-135 (D-22a): every binary16 pattern decodes to its IEEE 754 value and re-encodes
//! unchanged, on every cell. `camas f32` runs every binary32 pattern in release. Nightly also
//! checks binary16 against the `f16` primitive.
#![cfg_attr(feature = "nightly", feature(f16, f32_from_f16))]

mod common;

use cbor_static::ErrorKind;
use common::{float_bits, float_encoded, halves, ieee_binary16};

/// Every binary16 pattern, or every 257th under Miri.
fn halves_to_check() -> impl Iterator<Item = u16> {
    (0..=u16::MAX).step_by(if cfg!(miri) { 257 } else { 1 })
}

#[test]
fn every_half_decodes_to_its_ieee_value_and_reencodes_unchanged() {
    for half in halves_to_check() {
        let encoded = [0xf9, half.to_be_bytes()[0], half.to_be_bytes()[1]];
        assert_eq!(float_bits(&encoded), ieee_binary16(half), "{half:04x}");
        let (out, len) = float_encoded(ieee_binary16(half));
        assert_eq!(out[..len], encoded, "{half:04x}");
    }
}

#[cfg(feature = "nightly")]
#[test]
fn every_half_matches_the_f16_primitive() {
    for half in halves_to_check() {
        let primitive = f64::from(f16::from_bits(half));
        let ours = f64::from_bits(float_bits(&[
            0xf9,
            half.to_be_bytes()[0],
            half.to_be_bytes()[1],
        ]));
        if primitive.is_nan() {
            assert!(ours.is_nan(), "{half:04x}");
        } else {
            assert_eq!(ours.to_bits(), primitive.to_bits(), "{half:04x}");
        }
    }
}

#[test]
fn every_half_round_trips_through_an_f16_field_from_any_exact_width() {
    for half in halves_to_check() {
        let value = cbor_static::F16::from_bits(half);
        let [high, low] = half.to_be_bytes();
        assert_eq!(common::encode(&value), [0xf9, high, low], "{half:04x}");
        assert_eq!(common::decode(&[0xf9, high, low]), Ok(value), "{half:04x}");
        let single = value.to_f32().to_bits().to_be_bytes();
        assert_eq!(
            common::decode(&[0xfa, single[0], single[1], single[2], single[3]]),
            Ok(value),
            "{half:04x}"
        );
        assert_eq!(
            cbor_static::F16::from_f32(value.to_f32()),
            Some(value),
            "{half:04x}"
        );
        let single = value.to_f32();
        if single.is_nan() {
            assert_eq!(
                single.to_bits(),
                u32::from(half >> 15) << 31 | 0xff << 23 | u32::from(half & 0x3ff) << 13,
                "{half:04x}"
            );
        } else {
            assert_eq!(
                f64::from(single).to_bits(),
                ieee_binary16(half),
                "{half:04x}"
            );
        }
    }
}

#[test]
fn an_f16_field_takes_only_floats_binary16_holds() {
    for (inexact, error) in [
        (b"\xfa\x3d\xcc\xcc\xcd".as_slice(), ErrorKind::OutOfRange),
        (b"\xfa\x47\x80\x00\x00", ErrorKind::OutOfRange),
        (b"\xfb\x3f\xf0\0\0\0\0\0\x01", ErrorKind::OutOfRange),
        (b"\x01", ErrorKind::TypeMismatch),
    ] {
        assert_eq!(
            common::decode::<cbor_static::F16>(inexact),
            Err(error),
            "{inexact:x?}"
        );
    }
}

#[cfg(feature = "nightly")]
#[test]
fn the_f16_primitive_encodes_and_decodes_as_its_bits() {
    for half in halves_to_check() {
        let primitive = f16::from_bits(half);
        let ours = cbor_static::F16::from_bits(half);
        assert_eq!(
            common::encode(&primitive),
            common::encode(&ours),
            "{half:04x}"
        );
        assert_eq!(
            common::decode::<f16>(&common::encode(&ours)).map(f16::to_bits),
            Ok(half),
            "{half:04x}"
        );
        if primitive.is_nan() {
            assert!(ours.to_f32().is_nan(), "{half:04x}");
        } else {
            assert_eq!(
                ours.to_f32().to_bits(),
                f32::from(primitive).to_bits(),
                "{half:04x}"
            );
        }
    }
}

#[test]
#[ignore = "all 2^32 binary32 patterns: `camas f32` runs it in release"]
fn every_single_decodes_to_its_ieee_value_and_reencodes_at_its_narrowest() {
    use std::num::NonZero;
    use std::thread;

    let halves = halves();
    let threads = thread::available_parallelism().map_or(1, NonZero::get);
    thread::scope(|scope| {
        for first in 0..threads {
            let halves = &halves;
            scope.spawn(move || {
                for single in (0..=u32::MAX).skip(first).step_by(threads) {
                    let bytes = single.to_be_bytes();
                    let encoded = [0xfa, bytes[0], bytes[1], bytes[2], bytes[3]];
                    let bits = float_bits(&encoded);
                    let host = f64::from(f32::from_bits(single));
                    if host.is_nan() {
                        assert_eq!(
                            bits,
                            u64::from(single >> 31) << 63
                                | 0x7ff << 52
                                | u64::from(single & 0x007f_ffff) << 29,
                            "{single:08x}"
                        );
                    } else {
                        assert_eq!(bits, host.to_bits(), "{single:08x}");
                    }
                    let (out, len) = float_encoded(bits);
                    match halves.get(&bits) {
                        Some(half) => assert_eq!(
                            out[..len],
                            [0xf9, half.to_be_bytes()[0], half.to_be_bytes()[1]],
                            "{single:08x}"
                        ),
                        None => assert_eq!(out[..len], encoded, "{single:08x}"),
                    }
                }
            });
        }
    });
}
