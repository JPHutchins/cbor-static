#![allow(dead_code, reason = "each test crate uses some of these helpers")]

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::{
    Decode, Decoder, Encode, Encoder, ErrorKind, Fed, Progress, decode_into, encode_into,
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
    let mut decoder = Decoder::new(&mut slot);
    for piece in bytes.chunks(chunk) {
        if let Fed::Done(_) = decoder.feed(piece).map_err(|e| e.kind)? {
            break;
        }
    }
    decoder.finish().map(|value| *value).map_err(|e| e.kind)
}
