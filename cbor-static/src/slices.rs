#![deny(unsafe_code)]
//! By-value decoding of a slice (R-203, R-205): convenience over [`crate::decode_into`], which
//! returns the value where a caller's slot already holds it (D-15).

use crate::engine::by_value;
use crate::error::Error;
use crate::schema::Decode;

/// Decodes exactly one item spanning all of `input`, and returns it.
///
/// ```
/// #[derive(cbor_static::Decode, Debug, PartialEq)]
/// struct Reading<'a> {
///     id: u32,
///     name: &'a str,
/// }
///
/// assert_eq!(
///     cbor_static::from_slice::<Reading>(b"\xa2\x62id\x07\x64name\x61a")?,
///     Reading { id: 7, name: "a" }
/// );
/// assert_eq!(
///     cbor_static::from_slice::<u8>(b"\x01\x02").map_err(|e| (e.kind, e.offset)),
///     Err((cbor_static::ErrorKind::TrailingInput, 1))
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`crate::decode_into`].
pub fn from_slice<'de, T: Decode<'de>>(input: &'de [u8]) -> Result<T, Error> {
    by_value(|slot| crate::decode_into(input, slot).map(|value| (value, ())))
        .map(|(value, ())| value)
}

/// Decodes the one item `input` starts with, and returns it with the bytes after it: the rest of
/// an RFC 8742 CBOR sequence.
///
/// ```
/// let (first, rest) = cbor_static::from_slice_partial::<u32>(b"\x01\x19\x01\xf4")?;
/// assert_eq!((first, rest), (1, b"\x19\x01\xf4".as_slice()));
/// assert_eq!(cbor_static::from_slice_partial::<u32>(rest)?, (500, [].as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`crate::decode_into`], save [`crate::ErrorKind::TrailingInput`]: a truncated item, or an
/// empty `input`, is [`crate::ErrorKind::UnexpectedEnd`].
pub fn from_slice_partial<'de, T: Decode<'de>>(input: &'de [u8]) -> Result<(T, &'de [u8]), Error> {
    use crate::engine::{Decoder, Fed};

    by_value(|slot| {
        let mut decoder = Decoder::new(slot);
        match decoder.feed(input) {
            Ok(Fed::Done(consumed)) => decoder
                .finish()
                .map(|value| (value, input.get(consumed..).unwrap_or_default())),
            fed => Err(crate::not_whole(fed, input)),
        }
    })
}
