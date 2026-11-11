//! Well-formedness (D-12, RFC 8949 Appendix F) across the three walkers that check it: `Tokens`,
//! the contiguous skip (`skip::item_len`), and the engine's streaming skip of an unknown field,
//! fed in chunks. Each is compared with a naive reference.

mod appendix_f;
mod model;
mod nesting;
mod oracle;

use core::mem::MaybeUninit;

use crate::engine::item_len;
use crate::error::ErrorKind;
use crate::{Decoder, Fed, Tokens};

/// Why an input is not well-formed, by Appendix F's kinds: too little data, or a syntax error.
/// Invalid UTF-8 is its own class (D-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Truncated,
    Malformed,
    InvalidUtf8,
}

/// A map with one entry, `x`, which `Probe` does not know and so skips.
#[derive(crate::Decode)]
struct Probe {
    #[serde(skip_serializing_if = "Option::is_none")]
    _known: Option<u32>,
}

const PROBE_PREFIX: [u8; 3] = *b"\xa1\x61x";

impl From<ErrorKind> for Class {
    fn from(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::NotWellFormed => Self::Malformed,
            ErrorKind::InvalidUtf8 => Self::InvalidUtf8,
            _ => Self::Truncated,
        }
    }
}

/// `Tokens` over all of `bytes` as a CBOR sequence.
fn tokens(bytes: &[u8]) -> Result<(), Class> {
    Tokens::new(bytes)
        .try_for_each(|token| token.map(|_| ()))
        .map_err(|e| e.kind.into())
}

/// The contiguous skip of the first item of `bytes`.
fn skipped(bytes: &[u8]) -> Result<usize, Class> {
    item_len(bytes).map_err(Class::from)
}

/// The streaming skip of the first item of `bytes` as `Probe`'s unknown field, fed `chunk` bytes
/// at a time: the item's length, or the error.
fn streamed(bytes: &[u8], chunk: usize) -> Result<usize, ErrorKind> {
    let input = [PROBE_PREFIX.as_slice(), bytes].concat();
    let mut slot = MaybeUninit::<Probe>::uninit();
    let mut decoder = Decoder::new(&mut slot);
    let mut fed = 0;
    for piece in input.chunks(chunk) {
        match decoder.feed(piece).map_err(|e| e.kind)? {
            Fed::Done(consumed) => {
                decoder.finish().map_err(|e| e.kind)?;
                return Ok(fed + consumed - PROBE_PREFIX.len());
            }
            Fed::NeedInput => fed += piece.len(),
        }
    }
    decoder.finish().map(|_| 0).map_err(|e| e.kind)
}

/// Asserts that every walker agrees with the reference on `bytes`: the item's length or class for
/// the contiguous walkers, and acceptance and length at every chunk size for the streaming one.
#[track_caller]
fn agree(bytes: &[u8], chunks: impl IntoIterator<Item = usize>) {
    let reference = oracle::item(bytes);
    assert_eq!(skipped(bytes), reference, "skip {bytes:02x?}");
    assert_eq!(
        tokens(bytes),
        oracle::sequence(bytes),
        "tokens {bytes:02x?}"
    );
    for chunk in chunks {
        assert_eq!(
            streamed(bytes, chunk).ok(),
            reference.ok(),
            "stream in chunks of {chunk}: {bytes:02x?}"
        );
    }
}
