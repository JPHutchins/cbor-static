#![deny(unsafe_code)]
//! The loops both I/O trait families share (#5 Q6, Q16): an [`crate::Encoder`] resumed into a
//! scratch buffer the writer drains, and a [`crate::Decoder`] fed each read the reader fills into
//! it.

use core::ops::Range;

use crate::engine::{Fed, Progress};
use crate::error::{Error, ErrorKind};
use crate::schema::{Decode, Encode};

/// A failure of the reader or writer, or of the encoding or decoding itself.
///
/// ```
/// use cbor_static::{ErrorKind, IoError};
///
/// let decoded = cbor_static::from_slice::<u8>(b"\x18").map_err(IoError::<&str>::Cbor);
/// assert!(matches!(decoded, Err(IoError::Cbor(e)) if e.kind == ErrorKind::UnexpectedEnd));
/// assert!(matches!(IoError::<&str>::Io("reset"), IoError::Io("reset")));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoError<E> {
    Io(E),
    Cbor(Error),
}

/// Hands a filled part of the scratch buffer to the writer, given the bytes written before it.
type WriteAll<'w, E> = dyn FnMut(&[u8], usize) -> Result<(), IoError<E>> + 'w;
/// Fills the scratch buffer from the reader, returning how much it filled.
type Fill<'r, E> = dyn FnMut(&mut [u8]) -> Result<usize, E> + 'r;
type Resume<'v> = dyn FnMut(&mut [u8]) -> Result<Progress, Error> + 'v;
type Feed<'v> = dyn FnMut(&[u8]) -> Result<Fed, Error> + 'v;

/// Encodes `value` through `buf`, handing each filled part to `write_all` with the count written
/// before it, and returns the total.
pub(crate) fn encoded<T: Encode, E>(
    value: &T,
    buf: &mut [u8],
    write_all: &mut WriteAll<'_, E>,
) -> Result<usize, IoError<E>> {
    use crate::engine::Encoder;

    let mut encoder = Encoder::new(value);
    drained(&mut |out| encoder.resume(out), buf, write_all)
}

/// `encoded`'s loop, which no value type reaches but through `resume`.
#[inline(never)]
fn drained<E>(
    resume: &mut Resume<'_>,
    buf: &mut [u8],
    write_all: &mut WriteAll<'_, E>,
) -> Result<usize, IoError<E>> {
    let mut written = 0;
    loop {
        match resume(buf).map_err(IoError::Cbor)? {
            Progress::NeedSpace(0) => return Err(too_small(written)),
            Progress::NeedSpace(filled) => {
                write_all(buf.get(..filled).unwrap_or_default(), written)?;
                written += filled;
            }
            Progress::Done(filled) => {
                write_all(buf.get(..filled).unwrap_or_default(), written)?;
                return Ok(written + filled);
            }
        }
    }
}

/// Decodes one item from what `read` fills into `buf`, and returns it with the bytes of the last
/// read that follow it.
pub(crate) fn decoded<'b, T: for<'de> Decode<'de>, E>(
    buf: &'b mut [u8],
    read: &mut Fill<'_, E>,
) -> Result<(T, &'b [u8]), IoError<E>> {
    use crate::engine::{Decoder, by_value};

    let (value, rest) = by_value(
        |slot| {
            let mut decoder = Decoder::new(slot);
            let rest = filled(&mut |chunk| decoder.feed_transient(chunk), buf, read)?;
            decoder
                .finish()
                .map(|value| (value, rest))
                .map_err(IoError::Cbor)
        },
        IoError::Cbor,
    )?;
    Ok((
        value,
        rest.and_then(|rest| buf.get(rest)).unwrap_or_default(),
    ))
}

/// `decoded`'s loop, which no value type reaches but through `feed`: the bytes of the last read
/// after the item, or none when the reader ended first, which the decoder reports.
#[inline(never)]
fn filled<E>(
    feed: &mut Feed<'_>,
    buf: &mut [u8],
    read: &mut Fill<'_, E>,
) -> Result<Option<Range<usize>>, IoError<E>> {
    if buf.is_empty() {
        return Err(too_small(0));
    }
    loop {
        match read(buf).map_err(IoError::Io)? {
            0 => return Ok(None),
            end => {
                if let Fed::Done(used) =
                    feed(buf.get(..end).unwrap_or_default()).map_err(IoError::Cbor)?
                {
                    return Ok(Some(used..end));
                }
            }
        }
    }
}

/// An empty scratch buffer, or a writer that took none of a non-empty part `written` bytes in.
pub(crate) fn too_small<E>(written: usize) -> IoError<E> {
    IoError::Cbor(Error::new(ErrorKind::BufferTooSmall, 0, written))
}
