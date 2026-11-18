#![deny(unsafe_code)]
//! Encoding to and decoding from [`std::io::Write`] and [`std::io::Read`] (#5 Q16).

use std::io::{self, Read, Write};

use crate::schema::{Decode, Encode};
use crate::streams::IoError;

/// Encodes `value` to `writer` through the scratch `buf`, returning the bytes written. Nothing is
/// flushed.
///
/// ```
/// let mut out = Vec::new();
/// assert_eq!(cbor_static::io::to_writer(&[1u8, 2], &mut out, &mut [0u8; 2])?, 3);
/// assert_eq!(out, b"\x82\x01\x02");
/// # Ok::<(), cbor_static::IoError<std::io::Error>>(())
/// ```
///
/// # Errors
/// [`IoError::Io`] for the writer's error, [`std::io::ErrorKind::WriteZero`] included.
/// [`IoError::Cbor`] as [`crate::Encoder::resume`], or with [`crate::ErrorKind::BufferTooSmall`]
/// for an empty `buf`.
pub fn to_writer<T: Encode, W: Write>(
    value: &T,
    writer: &mut W,
    buf: &mut [u8],
) -> Result<usize, IoError<io::Error>> {
    use crate::streams::encoded;

    encoded(value, buf, &mut |part, _| {
        writer.write_all(part).map_err(IoError::Io)
    })
}

/// Decodes one item from `reader` through the scratch `buf`, returning it with the rest of the
/// last read: the start of the next item of an RFC 8742 sequence. Nothing past that read is read.
///
/// ```
/// let mut reader = std::io::Cursor::new(b"\x01\x19\x01\xf4");
/// let mut buf = [0u8; 8];
/// let (first, rest) = cbor_static::io::from_reader::<u16, _>(&mut reader, &mut buf)?;
/// assert_eq!((first, rest), (1, b"\x19\x01\xf4".as_slice()));
/// # Ok::<(), cbor_static::IoError<std::io::Error>>(())
/// ```
///
/// A type that borrows its input can't be read:
///
/// ```compile_fail
/// let mut reader = std::io::Cursor::new(b"\x61a");
/// let _ = cbor_static::io::from_reader::<&str, _>(&mut reader, &mut [0u8; 4]);
/// ```
///
/// # Errors
/// [`IoError::Io`] for the reader's error, save [`std::io::ErrorKind::Interrupted`], which is
/// retried. [`IoError::Cbor`] as [`crate::Decoder::feed`], or with
/// [`crate::ErrorKind::UnexpectedEnd`] at the bytes read when the reader ends first, offset 0 for
/// an empty stream, or [`crate::ErrorKind::BufferTooSmall`] for an empty `buf`.
pub fn from_reader<'b, T: for<'de> Decode<'de>, R: Read>(
    reader: &mut R,
    buf: &'b mut [u8],
) -> Result<(T, &'b [u8]), IoError<io::Error>> {
    use crate::streams::decoded;

    decoded(buf, &mut |chunk| {
        loop {
            match reader.read(chunk) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                read => return read,
            }
        }
    })
}
