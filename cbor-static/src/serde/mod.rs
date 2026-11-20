#![forbid(unsafe_code)]
//! serde's data model in CBOR (feature `serde`), in the bytes the native derive writes for the
//! same shapes (D-08): a struct is a map of text keys in declaration order, `None` and unit are
//! null, and an enum is externally tagged. Decoding returns by value only (D-15a).
//!
//! ```
//! #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
//! enum Shape {
//!     Empty,
//!     Circle { r: u32 },
//! }
//!
//! let mut out = [0u8; 32];
//! let len = cbor_static::serde::to_slice(&[Shape::Empty, Shape::Circle { r: 1 }], &mut out)?;
//! assert_eq!(&out[..len], b"\x82\x65Empty\xa1\x66Circle\xa1\x61r\x01");
//! assert_eq!(
//!     cbor_static::serde::from_slice::<[Shape; 2]>(&out[..len])?,
//!     [Shape::Empty, Shape::Circle { r: 1 }]
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```

mod de;
mod error;
mod ser;

use serde_core::{Deserialize, Serialize};

pub use de::Deserializer;
pub use error::Error;
pub use ser::{Compound, Serializer};

/// Encodes `value` into `out`, returning the bytes written.
///
/// ```
/// let mut out = [0u8; 3];
/// assert_eq!(cbor_static::serde::to_slice(&500u16, &mut out)?, 3);
/// assert_eq!(
///     cbor_static::serde::to_slice(&70_000u32, &mut out).map_err(|e| (e.kind, e.offset)),
///     Err((cbor_static::ErrorKind::BufferTooSmall, 3))
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// [`crate::ErrorKind::BufferTooSmall`] when `out` cannot hold the encoding,
/// [`crate::ErrorKind::NotWellFormed`] for a sequence or map whose items are not the number it
/// declared, and any error the value's serde impl raises, at the offset reached.
pub fn to_slice<T: ?Sized + Serialize>(value: &T, out: &mut [u8]) -> Result<usize, crate::Error> {
    let mut serializer = Serializer::new(out);
    match value.serialize(&mut serializer) {
        Ok(()) => Ok(serializer.written()),
        Err(error) => Err(error.reported(serializer.written())),
    }
}

/// `value`'s encoding, in a vector of exactly its length (feature `alloc`).
///
/// ```
/// let encoded = cbor_static::serde::to_vec(&("a", 1u8))?;
/// assert_eq!(encoded, b"\x82\x61a\x01");
/// assert_eq!(encoded.capacity(), encoded.len());
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// [`crate::ErrorKind::OutOfMemory`] when the allocator refuses the vector, through the
/// allocation [`crate::to_vec_in`] also makes, and as [`to_slice`].
#[cfg(feature = "alloc")]
pub fn to_vec<T: ?Sized + Serialize>(value: &T) -> Result<alloc::vec::Vec<u8>, crate::Error> {
    let mut measuring = Serializer::measuring();
    let len = match value.serialize(&mut measuring) {
        Ok(()) => measuring.written(),
        Err(error) => return Err(error.reported(measuring.written())),
    };
    let mut out = alloc::vec::Vec::new();
    let written = to_slice(
        value,
        crate::engine::zeroed(&mut out, len).map_err(crate::Error::from)?,
    )?;
    out.truncate(written);
    Ok(out)
}

/// Decodes exactly one item spanning all of `input`, borrowing its definite strings.
///
/// ```
/// #[derive(serde::Deserialize, Debug, PartialEq)]
/// struct Reading<'a> {
///     id: u32,
///     name: &'a str,
/// }
///
/// assert_eq!(
///     cbor_static::serde::from_slice::<Reading>(b"\xa2\x62id\x07\x64name\x61a")?,
///     Reading { id: 7, name: "a" }
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`Deserializer::decode`].
pub fn from_slice<'de, T: Deserialize<'de>>(input: &'de [u8]) -> Result<T, crate::Error> {
    Deserializer::new(input).decode()
}

/// Decodes the first item of `input`, returning the bytes after it (RFC 8742).
///
/// ```
/// assert_eq!(cbor_static::serde::from_slice_partial::<u8>(b"\x01\x02")?, (1, b"\x02".as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`Deserializer::decode_partial`].
pub fn from_slice_partial<'de, T: Deserialize<'de>>(
    input: &'de [u8],
) -> Result<(T, &'de [u8]), crate::Error> {
    Deserializer::new(input).decode_partial()
}
