#![no_std]
//! RFC 8949 CBOR for `no_std`: derives emit constant descriptors, and one non-generic,
//! iterative, resumable engine walks them.
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Reading<'a> {
//!     id: u32,
//!     #[serde(skip_serializing_if = "Option::is_none")]
//!     rssi: Option<u32>,
//!     name: &'a str,
//! }
//!
//! let mut out = [0u8; 32];
//! let len = cbor_static::encode_into(&Reading { id: 7, rssi: None, name: "a" }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x62id\x07\x64name\x61a");
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! let decoded: &mut Reading = cbor_static::decode_into(&out[..len], &mut slot)?;
//! assert_eq!(decoded, &Reading { id: 7, rssi: None, name: "a" });
//! # Ok::<(), cbor_static::Error>(())
//! ```

extern crate self as cbor_static;
#[cfg(test)]
extern crate std;

mod engine;
mod error;
mod scalar;
mod schema;
mod stack;
mod wire;

use core::mem::MaybeUninit;

pub use cbor_static_derive::{Cbor, Decode, Encode};
pub use engine::{Decoder, Encoder, Fed, Progress};
pub use error::{Error, ErrorKind};
pub use scalar::Tagged;
pub use schema::{Decode, Encode};

/// Encodes `value` into `out` in one call, returning the bytes written.
///
/// # Errors
/// [`ErrorKind::BufferTooSmall`] when `out` cannot hold the whole encoding.
pub fn encode_into<T: Encode>(value: &T, out: &mut [u8]) -> Result<usize, Error> {
    match Encoder::new(value).resume(out)? {
        Progress::Done(written) => Ok(written),
        Progress::NeedSpace(written) => Err(Error::new(ErrorKind::BufferTooSmall, 0, written)),
    }
}

/// Decodes exactly one item spanning all of `input` into `slot`.
///
/// # Errors
/// Any [`ErrorKind`]: malformed or truncated input, a type or range mismatch, a missing or
/// duplicate field, or bytes left over after the item.
pub fn decode_into<'de, 's, T: Decode<'de>>(
    input: &'de [u8],
    slot: &'s mut MaybeUninit<T>,
) -> Result<&'s mut T, Error> {
    let mut decoder = Decoder::new(slot);
    match decoder.feed(input) {
        Ok(Fed::Done(consumed)) if consumed == input.len() => decoder.finish(),
        Ok(Fed::Done(consumed)) => Err(Error::new(ErrorKind::TrailingInput, 0, consumed)),
        Ok(Fed::NeedInput) => Err(Error::new(ErrorKind::UnexpectedEnd, 0, input.len())),
        Err(error) if error.kind == ErrorKind::NeedContiguous => Err(Error {
            kind: ErrorKind::UnexpectedEnd,
            ..error
        }),
        Err(error) => Err(error),
    }
}

#[doc(hidden)]
pub mod __private {
    pub use crate::engine::{
        Pending, assemble, drop_fn, field_ptr_of, index_of, index_tag, offset, write_tag,
    };
    pub use crate::schema::{
        Assemble, DecKind, DecVariant, EncKind, EnumGlue, FieldLayout, Layout,
    };
    pub use crate::stack::{Level, Widest, deeper};
}
