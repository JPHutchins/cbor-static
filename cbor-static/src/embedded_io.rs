#![deny(unsafe_code)]
//! Encoding to and decoding from `embedded-io` 0.7's blocking [`Write`] and [`Read`] (#5 Q16).

use ::embedded_io::{Read, Write};

use crate::schema::{Decode, Encode};
use crate::streams::IoError;

/// Encodes `value` to `writer` through the scratch `buf`, returning the bytes written. Nothing is
/// flushed.
///
/// ```
/// let mut out = [0u8; 8];
/// let mut writer = out.as_mut_slice();
/// assert_eq!(
///     cbor_static::embedded_io::to_writer(&("hi", 500u16), &mut writer, &mut [0u8; 2])?,
///     7
/// );
/// assert_eq!(out[..7], *b"\x82\x62hi\x19\x01\xf4");
/// # Ok::<(), cbor_static::IoError<embedded_io::SliceWriteError>>(())
/// ```
///
/// # Errors
/// [`IoError::Io`] for the writer's error. [`IoError::Cbor`] as [`crate::Encoder::resume`], or
/// with [`crate::ErrorKind::BufferTooSmall`] for an empty `buf`, or for a writer that accepts none
/// of a non-empty part, at the bytes written before it.
pub fn to_writer<T: Encode, W: Write>(
    value: &T,
    writer: &mut W,
    buf: &mut [u8],
) -> Result<usize, IoError<W::Error>> {
    use crate::streams::{encoded, too_small};

    encoded(value, buf, &mut |mut part, mut written| {
        while !part.is_empty() {
            match writer.write(part).map_err(IoError::Io)? {
                0 => return Err(too_small(written)),
                taken => {
                    part = part.get(taken..).unwrap_or_default();
                    written += taken;
                }
            }
        }
        Ok(())
    })
}

/// Decodes one item from `reader` through the scratch `buf`, returning it with the rest of the
/// last read: the start of the next item of an RFC 8742 sequence. Nothing past that read is read.
///
/// ```
/// let mut reader = b"\x82\x01\x02\x63abc".as_slice();
/// let mut buf = [0u8; 4];
/// let (pair, rest) = cbor_static::embedded_io::from_reader::<(u8, u8), _>(&mut reader, &mut buf)?;
/// assert_eq!((pair, rest), ((1, 2), b"\x63".as_slice()));
/// # Ok::<(), cbor_static::IoError<core::convert::Infallible>>(())
/// ```
///
/// A type that borrows its input can't be read:
///
/// ```compile_fail
/// let mut reader = b"\x61a".as_slice();
/// let _ = cbor_static::embedded_io::from_reader::<&str, _>(&mut reader, &mut [0u8; 4]);
/// ```
///
/// # Errors
/// [`IoError::Io`] for the reader's error. [`IoError::Cbor`] as [`crate::Decoder::feed`], or with
/// [`crate::ErrorKind::UnexpectedEnd`] at the bytes read when the reader ends first, offset 0 for
/// an empty stream, or [`crate::ErrorKind::BufferTooSmall`] for an empty `buf`.
pub fn from_reader<'b, T: for<'de> Decode<'de>, R: Read>(
    reader: &mut R,
    buf: &'b mut [u8],
) -> Result<(T, &'b [u8]), IoError<R::Error>> {
    use crate::streams::decoded;

    decoded(buf, &mut |chunk| reader.read(chunk))
}
