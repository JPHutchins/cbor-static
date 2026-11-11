//! R-103: wire lengths become `usize` through one checked conversion, which `--cfg
//! cbor_static_usize16` caps at `u16::MAX`. Streaming skips never convert, so they pass any length.

mod common;

use cbor_static::{ErrorKind, encode_into};
use common::{chunked_decode, decode};

const LIMIT: usize = u16::MAX as usize;

#[derive(cbor_static::Cbor, Clone, Copy, Debug, PartialEq)]
struct Probe {
    #[serde(skip_serializing_if = "Option::is_none")]
    a: Option<u32>,
}

#[derive(cbor_static::Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "t")]
enum Kind {
    A,
}

#[test]
fn strings_at_the_16_bit_limit_decode() {
    assert_eq!(
        decode::<&[u8]>(&encoded(&vec![0; LIMIT].as_slice())),
        Ok(vec![0; LIMIT].as_slice())
    );
    assert_eq!(
        decode::<&str>(&encoded(&"a".repeat(LIMIT).as_str())),
        Ok("a".repeat(LIMIT).as_str())
    );
}

#[test]
fn strings_one_past_the_16_bit_limit_exceed_it_only_under_usize16() {
    assert_eq!(
        decode::<&[u8]>(&encoded(&vec![0; LIMIT + 1].as_slice())),
        beyond_16_bits(vec![0; LIMIT + 1].as_slice())
    );
    assert_eq!(
        decode::<&str>(&encoded(&"a".repeat(LIMIT + 1).as_str())),
        beyond_16_bits("a".repeat(LIMIT + 1).as_str())
    );
}

#[test]
fn streaming_skips_pass_any_length() {
    let input = [
        b"\xa1\x61x".as_slice(),
        &encoded(&vec![0; LIMIT + 1].as_slice()),
    ]
    .concat();
    assert_eq!(decode::<Probe>(&input), Ok(Probe { a: None }));
    for chunk in [7, 4096] {
        assert_eq!(
            chunked_decode::<Probe>(&input, chunk),
            Ok(Probe { a: None }),
            "{chunk}"
        );
    }
}

#[test]
fn contiguous_scans_convert_every_length() {
    let tagged = |len: usize| {
        [
            b"\xa2\x61x".as_slice(),
            &encoded(&vec![0; len].as_slice()),
            b"\x61t\x61A",
        ]
        .concat()
    };
    assert_eq!(decode::<Kind>(&tagged(LIMIT)), Ok(Kind::A));
    assert_eq!(decode::<Kind>(&tagged(LIMIT + 1)), beyond_16_bits(Kind::A));
}

fn beyond_16_bits<T>(decoded: T) -> Result<T, ErrorKind> {
    if cfg!(cbor_static_usize16) {
        Err(ErrorKind::LimitExceeded)
    } else {
        Ok(decoded)
    }
}

fn encoded<T: cbor_static::Encode>(value: &T) -> Vec<u8> {
    let mut out = vec![0; 2 * LIMIT + 16];
    let len = encode_into(value, &mut out).expect("encodes");
    out.truncate(len);
    out
}
