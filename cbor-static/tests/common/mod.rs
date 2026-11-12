#![allow(dead_code, reason = "each test crate uses some of these helpers")]

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::{
    Decode, Decoder, Encode, Encoder, ErrorKind, Fed, Progress, decode_into, decode_strict_into,
    encode_into,
};

/// R-009's correctness criterion for an owned type: `value` encodes to exactly `bytes`, whole and
/// in every output chunk size up to 64, and `bytes` decodes to `value`, whole and in every input
/// chunk size up to 64.
pub fn golden<T>(value: &T, bytes: &[u8])
where
    T: Encode + for<'de> Decode<'de> + Copy + PartialEq + Debug,
{
    assert_eq!(encode(value), bytes, "encode");
    assert_eq!(decode::<T>(bytes), Ok(*value), "decode");
    for chunk in 1..=64 {
        assert_eq!(
            chunked_encode(value, chunk),
            bytes,
            "encode in chunks of {chunk}"
        );
        assert_eq!(
            chunked_decode::<T>(bytes, chunk),
            Ok(*value),
            "decode in chunks of {chunk}"
        );
    }
}

/// `golden` for a type that is not `Copy`, compared where it was decoded.
pub fn golden_in_place<T>(value: &T, bytes: &[u8])
where
    T: Encode + for<'de> Decode<'de> + PartialEq + Debug,
{
    assert_eq!(encode(value), bytes, "encode");
    for chunk in 1..=64 {
        assert_eq!(
            chunked_encode(value, chunk),
            bytes,
            "encode in chunks of {chunk}"
        );
        let mut slot = MaybeUninit::uninit();
        let mut decoder = Decoder::<T>::new(&mut slot);
        for piece in bytes.chunks(chunk) {
            if let Ok(Fed::Done(_)) = decoder.feed(piece) {
                break;
            }
        }
        assert_eq!(
            decoder.finish().map(|decoded| &*decoded == value),
            Ok(true),
            "decode in chunks of {chunk}"
        );
    }
}

/// `golden` for internally and adjacently tagged values, whose decoding needs contiguous input
/// (D-10, D-16): encoding still resumes at every chunk size, and decoding in chunks either
/// succeeds or reports `NeedContiguous`.
pub fn golden_contiguous<T>(value: &T, bytes: &[u8])
where
    T: Encode + for<'de> Decode<'de> + Copy + PartialEq + Debug,
{
    assert_eq!(encode(value), bytes, "encode");
    assert_eq!(decode::<T>(bytes), Ok(*value), "decode");
    for chunk in 1..=64 {
        assert_eq!(
            chunked_encode(value, chunk),
            bytes,
            "encode in chunks of {chunk}"
        );
        assert!(
            matches!(
                chunked_decode::<T>(bytes, chunk),
                Err(ErrorKind::NeedContiguous)
            ) || chunked_decode::<T>(bytes, chunk) == Ok(*value),
            "decode in chunks of {chunk}"
        );
    }
    assert_eq!(chunked_decode::<T>(bytes, bytes.len()), Ok(*value));
}

pub fn encode<T: Encode>(value: &T) -> Vec<u8> {
    let mut out = [0u8; 512];
    let len = encode_into(value, &mut out).expect("encodes");
    out[..len].to_vec()
}

pub fn decode<'de, T: Decode<'de> + Copy>(bytes: &'de [u8]) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    decode_into(bytes, &mut slot)
        .map(|value| *value)
        .map_err(|e| e.kind)
}

pub fn decode_strict<'de, T: Decode<'de> + Copy>(bytes: &'de [u8]) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    decode_strict_into(bytes, &mut slot)
        .map(|value| *value)
        .map_err(|e| e.kind)
}

pub fn chunked_encode<T: Encode>(value: &T, chunk: usize) -> Vec<u8> {
    let mut encoder = Encoder::new(value);
    let mut buffer = vec![0u8; chunk];
    let mut out = Vec::new();
    loop {
        match encoder.resume(&mut buffer).expect("encodes") {
            Progress::Done(n) => return [out, buffer[..n].to_vec()].concat(),
            Progress::NeedSpace(n) => out.extend_from_slice(&buffer[..n]),
        }
    }
}

pub fn chunked_decode<'de, T: Decode<'de> + Copy>(
    bytes: &'de [u8],
    chunk: usize,
) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    fed(&mut Decoder::new(&mut slot), bytes, chunk)
}

pub fn chunked_decode_strict<'de, T: Decode<'de> + Copy>(
    bytes: &'de [u8],
    chunk: usize,
) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    fed(&mut Decoder::new_strict(&mut slot), bytes, chunk)
}

fn fed<'de, T: Decode<'de> + Copy>(
    decoder: &mut Decoder<'de, '_, T>,
    bytes: &'de [u8],
    chunk: usize,
) -> Result<T, ErrorKind> {
    for piece in bytes.chunks(chunk) {
        if let Fed::Done(_) = decoder.feed(piece).map_err(|e| e.kind)? {
            break;
        }
    }
    decoder.finish().map(|value| *value).map_err(|e| e.kind)
}

/// The binary64 bits of an IEEE 754 binary16 pattern, from the format's definition: exact f64
/// arithmetic for numbers, and the payload moved into place for NaNs.
pub fn ieee_binary16(half: u16) -> u64 {
    let sign = if half >> 15 == 1 { -1.0 } else { 1.0 };
    let fraction = f64::from(half & 0x3ff);
    match (half >> 10 & 0x1f, half & 0x3ff) {
        (0x1f, 0) => (sign * f64::INFINITY).to_bits(),
        (0x1f, payload) => u64::from(half >> 15) << 63 | 0x7ff << 52 | u64::from(payload) << 42,
        (0, _) => (sign * fraction * power_of_two(-24)).to_bits(),
        (exponent, _) => {
            (sign * (1.0 + fraction / 1024.0) * power_of_two(i32::from(exponent) - 15)).to_bits()
        }
    }
}

/// 2^`exponent` from its binary64 exponent field. `powi` need not be exact, and Miri perturbs it.
fn power_of_two(exponent: i32) -> f64 {
    f64::from_bits(u64::try_from(exponent + 1023).expect("a normal exponent") << 52)
}

/// Every binary16 pattern, keyed by the binary64 bits of its value.
pub fn halves() -> std::collections::HashMap<u64, u16> {
    (0..=u16::MAX)
        .map(|half| (ieee_binary16(half), half))
        .collect()
}

/// The binary64 bits of the one float token `encoded` holds.
pub fn float_bits(encoded: &[u8]) -> u64 {
    match cbor_static::Tokens::new(encoded).next() {
        Some(Ok(cbor_static::Token::Float(value))) => value.to_bits(),
        other => panic!("{encoded:x?}: {other:?}"),
    }
}

/// The preferred encoding of a float token with the binary64 `bits`.
pub fn float_encoded(bits: u64) -> ([u8; 9], usize) {
    let mut out = [0u8; 9];
    let mut writer = cbor_static::TokenWriter::new(&mut out);
    writer
        .write(cbor_static::Token::Float(f64::from_bits(bits)))
        .expect("fits");
    let len = writer.written();
    (out, len)
}
