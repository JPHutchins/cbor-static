#![no_std]
#![cfg_attr(feature = "nightly", feature(f16))]
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
//!
//! Integer and enum keys, negative ones included, and `deny_unknown_fields` (D-09, D-12):
//!
//! ```
//! #[derive(Clone, Copy)]
//! enum Label {
//!     Alg = 1,
//!     Crv = -1,
//! }
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[cbor(keys = Label, deny_unknown_fields)]
//! struct Key {
//!     alg: i32,
//!     crv: u32,
//! }
//!
//! let mut out = [0u8; 8];
//! let len = cbor_static::encode_into(&Key { alg: -7, crv: 1 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x01\x26\x20\x01");
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     cbor_static::decode_into::<Key>(b"\xa2\x01\x26\x02\x01", &mut slot).map_err(|e| e.kind),
//!     Err(cbor_static::ErrorKind::UnknownField)
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! A field maps to the `keys` variant its name gives in `UpperCamelCase`, so `key_ops` maps to
//! `KeyOps` (R-122). A field without its variant does not compile:
//!
//! ```compile_fail,E0599
//! #[derive(Clone, Copy)]
//! enum Label {
//!     Alg = 1,
//! }
//!
//! #[derive(cbor_static::Cbor)]
//! #[cbor(keys = Label)]
//! struct Key {
//!     alg: i32,
//!     key_ops: u32,
//! }
//! ```
//!
//! Nor does a variant without its field:
//!
//! ```compile_fail,E0004
//! #[derive(Clone, Copy)]
//! enum Label {
//!     Alg = 1,
//!     KeyOps = 4,
//! }
//!
//! #[derive(cbor_static::Cbor)]
//! #[cbor(keys = Label)]
//! struct Key {
//!     alg: i32,
//! }
//! ```
//!
//! Positional structs, whose trailing `None`s are truncated, and tuples (D-09, D-20):
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[cbor(array)]
//! struct Point {
//!     at: (i32, i32),
//!     label: Option<u32>,
//! }
//!
//! let mut out = [0u8; 8];
//! let len = cbor_static::encode_into(&Point { at: (1, -2), label: None }, &mut out)?;
//! assert_eq!(&out[..len], b"\x81\x82\x01\x21");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! Unit, newtype and tuple structs take serde's shapes: null, the field, and an array of every
//! field (D-08):
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Ping;
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Meters(u32);
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Span(u32, Option<u32>);
//!
//! let mut out = [0u8; 8];
//! let len = cbor_static::encode_into(&(Ping, Meters(1000), Span(1, None)), &mut out)?;
//! assert_eq!(&out[..len], b"\x83\xf6\x19\x03\xe8\x82\x01\xf6");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! Their fields are positional, so a key attribute does not compile:
//!
//! ```compile_fail
//! #[derive(cbor_static::Cbor)]
//! struct Meters(#[serde(rename = "m")] u32);
//! ```
//!
//! A newtype variant's field is positional too: it takes `#[cbor(bytes)]` and nothing else.
//!
//! ```
//! #[derive(cbor_static::Cbor)]
//! enum Frame {
//!     Raw(#[cbor(bytes)] [u8; 4]),
//! }
//!
//! let mut out = [0u8; 16];
//! let len = cbor_static::encode_into(&Frame::Raw([1, 2, 3, 4]), &mut out)?;
//! assert_eq!(&out[..len], b"\xa1\x63Raw\x44\x01\x02\x03\x04");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! ```compile_fail
//! #[derive(cbor_static::Cbor)]
//! enum Frame {
//!     Raw(#[serde(rename = "r")] [u8; 4]),
//! }
//! ```
//!
//! An `Option` of a nested type is null or the value (D-08, D-20):
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Inner {
//!     x: u32,
//! }
//!
//! let mut out = [0u8; 8];
//! let len = cbor_static::encode_into(&[Some(Inner { x: 1 }), None], &mut out)?;
//! assert_eq!(&out[..len], b"\x82\xa1\x61x\x01\xf6");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! Untagged enums whose variants start with different items, picked by the initial byte (D-10):
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[serde(untagged)]
//! enum Id<'a> {
//!     Number(u64),
//!     Name(&'a str),
//! }
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(*cbor_static::decode_into::<Id>(b"\x61a", &mut slot)?, Id::Name("a"));
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! Integer-indexed and CBOR-tag-discriminated enums (D-10):
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[cbor(index)]
//! enum Command {
//!     Ping,
//!     Set(u32),
//! }
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! enum Envelope {
//!     #[cbor(cbor_tag = 18)]
//!     Signed(u32),
//!     #[cbor(cbor_tag = 17)]
//!     Maced(u32),
//! }
//!
//! let mut out = [0u8; 8];
//! let len = cbor_static::encode_into(&(Command::Set(5), Envelope::Maced(1)), &mut out)?;
//! assert_eq!(&out[..len], b"\x82\x82\x01\x05\xd1\x01");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! A `repr(u8)` or `repr(C, u8)` enum with implicit discriminants decodes in place through tables
//! (D-21), whose payload offsets come from shadow structs laid out as RFC 2195 lays out the
//! variants. With `#[cbor(offset_of_enum)]` they come from `offset_of!` on the enum itself, which
//! needs nightly and `#![feature(offset_of_enum)]` in the deriving crate (R-129); without it:
//!
//! ```compile_fail,E0658
//! #[derive(cbor_static::Cbor)]
//! #[repr(u8)]
//! #[cbor(offset_of_enum)]
//! enum Frame {
//!     Data(u32),
//! }
//! ```
//!
//! Internally and adjacently tagged enums, whose decoding needs contiguous input (D-10, D-16):
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[serde(tag = "type")]
//! enum Event {
//!     Start,
//!     Move { x: i32 },
//! }
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[serde(tag = "t", content = "c")]
//! enum Message {
//!     Data(u32),
//! }
//!
//! let mut out = [0u8; 32];
//! let len = cbor_static::encode_into(&Event::Move { x: 1 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x64type\x64Move\x61x\x01");
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     *cbor_static::decode_into::<Message>(b"\xa2\x61c\x07\x61t\x64Data", &mut slot)?,
//!     Message::Data(7)
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! Variants that can start with the same item do not compile:
//!
//! ```compile_fail,E0080
//! #[derive(cbor_static::Cbor)]
//! #[serde(untagged)]
//! enum Ambiguous {
//!     Small(u32),
//!     Large(u64),
//! }
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! let _ = cbor_static::decode_into::<Ambiguous>(b"\x01", &mut slot);
//! ```
//!
//! The serde attributes beyond names and keys (`rename_all`, `default`, `skip`, `transparent`,
//! `borrow`) are described in [`attributes`], and deterministic encoding in [`deterministic`].

extern crate self as cbor_static;
#[cfg(test)]
extern crate std;

pub mod attributes;
#[cfg(feature = "heapless")]
mod bounded;
mod containers;
pub mod deterministic;
mod engine;
mod error;
mod half;
mod keys;
mod lead;
mod scalar;
mod schema;
mod stack;
mod tags;
mod token;
#[cfg(test)]
mod well_formed;
mod wire;

use core::mem::MaybeUninit;

pub use cbor_static_derive::{Cbor, Decode, Encode};
pub use engine::{
    ArrayIter, ArrayRef, Chunk, ChunkIter, Chunks, Decoder, Encoder, Fed, MapIter, MapRef,
    Progress, RawCbor,
};
pub use error::{Error, ErrorKind};
pub use half::F16;
pub use schema::{ByteString, Decode, Encode};
pub use tags::{DateTime, Epoch, SelfDescribed, Tagged};
pub use token::{Token, TokenWriter, Tokens};

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
    whole(&mut Decoder::new(slot), input)
}

/// [`decode_into`] under the `Strict` profile ([`Decoder::new_strict`]).
///
/// ```
/// #[derive(cbor_static::Decode, Debug, PartialEq)]
/// struct Reading {
///     id: u32,
///     ok: bool,
/// }
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(
///     *cbor_static::decode_strict_into::<Reading>(b"\xa2\x62id\x07\x62ok\xf5", &mut slot)?,
///     Reading { id: 7, ok: true }
/// );
/// assert_eq!(
///     cbor_static::decode_strict_into::<Reading>(b"\xa2\x62ok\xf5\x62id\x07", &mut slot)
///         .map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::NotDeterministic)
/// );
/// assert_eq!(
///     cbor_static::decode_strict_into::<Reading>(b"\xa3\x61z\x00\x62id\x07\x62ok\xf5", &mut slot)
///         .map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::UnknownField)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`decode_into`], plus [`ErrorKind::NotDeterministic`] for anything RFC 8949 §4.2.1's
/// deterministic encoding would not write, and [`ErrorKind::UnknownField`] for an entry no field
/// claims.
pub fn decode_strict_into<'de, 's, T: Decode<'de>>(
    input: &'de [u8],
    slot: &'s mut MaybeUninit<T>,
) -> Result<&'s mut T, Error> {
    whole(&mut Decoder::new_strict(slot), input)
}

/// Feeds `input` to `decoder` as one chunk that must hold exactly one item.
fn whole<'de, 's, T: Decode<'de>>(
    decoder: &mut Decoder<'de, 's, T>,
    input: &'de [u8],
) -> Result<&'s mut T, Error> {
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
        BYTEWISE, DECODE_ADJACENT, DECODE_ARRAY, DECODE_CBOR_TAG, DECODE_EXTERNAL, DECODE_INDEX,
        DECODE_INTERNAL, DECODE_MAP, DECODE_MAP_DENY, DECODE_MAP_INT, DECODE_MAP_INT_DENY,
        DECODE_MAP_MIXED, DECODE_MAP_MIXED_DENY, DECODE_UNTAGGED, DeterministicOps,
        ENCODE_ADJACENT, ENCODE_ARRAY, ENCODE_CBOR_TAG, ENCODE_EXTERNAL, ENCODE_INDEX,
        ENCODE_INTERNAL, ENCODE_MAP, ENCODE_SKIPPED, ENCODE_UNTAGGED, LENGTH_FIRST, Pending,
        assemble, drop_fn, field_ptr_of, index_of, index_tag, offset, wraps, write_default,
        write_made, write_tag,
    };
    pub use crate::keys::{Key, field, map_ops, names, names_len, names_text, permuted, sorted};
    pub use crate::lead::Lead;
    pub use crate::schema::{
        Assemble, DecKind, DecVariant, Decoding, EncKind, Encoding, EnumGlue, FieldLayout, Layout,
        MakeDefault, MapForm, Order, map_form,
    };
    pub use crate::stack::{Level, Widest, deeper};
}
