//! Strings borrowed whole or in the chunks of an indefinite length (D-13, R-115). The chunk scan is
//! reached only through the kinds that accept chunks, so other firmware links none of it (D-22).
#![expect(
    clippy::cast_ptr_alignment,
    reason = "every field pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use core::fmt;
use core::marker::PhantomData;

use super::{Item, Next};
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::schema::{DecKind, Decode, EncKind, Encode};
use crate::stack::Level;
use crate::wire::Major;

/// A text (`Chunks<'de, str>`) or byte (`Chunks<'de, [u8]>`) string borrowed from the input, as the
/// chunks it arrived in: one for a definite string, any number for an indefinite one. Every chunk
/// is checked as it is decoded, UTF-8 included, so iterating cannot fail. Decoding an indefinite
/// one needs contiguous input, and encoding writes one definite string.
///
/// ```
/// use cbor_static::Chunks;
///
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Note<'a> {
///     text: Chunks<'a, str>,
///     data: Chunks<'a, [u8]>,
/// }
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let note: &mut Note =
///     cbor_static::decode_into(b"\xa2\x64text\x7f\x62ab\x61c\xff\x64data\x42\x01\x02", &mut slot)?;
/// assert!(note.text.into_iter().eq(["ab", "c"]));
/// assert!(note.data.into_iter().eq([[1u8, 2].as_slice()]));
///
/// let mut out = [0u8; 32];
/// let len = cbor_static::encode_into(&*note, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x64text\x63abc\x64data\x42\x01\x02");
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub struct Chunks<'de, T: ?Sized> {
    pub(super) span: Span<'de>,
    chunk: PhantomData<&'de T>,
}

/// A string's payload: a definite string's own, or an indefinite one's chunks, each with its head,
/// without the break.
#[derive(Clone, Copy)]
pub(super) enum Span<'de> {
    Definite(&'de [u8]),
    Indefinite(&'de [u8]),
}

/// The string types [`Chunks`] holds: `str` and `[u8]`.
///
/// ```compile_fail,E0277
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let _ = cbor_static::decode_into::<cbor_static::Chunks<[u16]>>(b"\x40", &mut slot);
/// ```
pub trait Chunk: sealed::Chunk {}

impl Chunk for str {}
impl Chunk for [u8] {}

pub(super) mod sealed {
    use super::{EncKind, Lead, Major};
    use crate::engine::{BYTE_CHUNKS, TEXT_CHUNKS};

    pub trait Chunk {
        const MAJOR: Major;
        const LEAD: Lead;
        const ENCODE: &'static EncKind;

        /// # Safety
        /// `chunk` is a chunk of a string of [`Chunk::MAJOR`], checked when it was decoded.
        unsafe fn assume(chunk: &[u8]) -> &Self;
    }

    impl Chunk for str {
        const MAJOR: Major = Major::Text;
        const LEAD: Lead = Lead::TEXT;
        const ENCODE: &'static EncKind = &EncKind::Chunks { ops: &TEXT_CHUNKS };

        unsafe fn assume(chunk: &[u8]) -> &Self {
            unsafe { core::str::from_utf8_unchecked(chunk) }
        }
    }

    impl Chunk for [u8] {
        const MAJOR: Major = Major::Bytes;
        const LEAD: Lead = Lead::BYTES;
        const ENCODE: &'static EncKind = &EncKind::Chunks { ops: &BYTE_CHUNKS };

        unsafe fn assume(chunk: &[u8]) -> &Self {
            chunk
        }
    }
}

/// The chunks of a [`Chunks`], in order.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let text = cbor_static::decode_into::<cbor_static::Chunks<str>>(b"\x7f\x61a\x60\xff", &mut slot)?;
/// let mut chunks = text.into_iter();
/// assert_eq!([chunks.next(), chunks.next(), chunks.next()], [Some("a"), Some(""), None]);
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub struct ChunkIter<'de, T: ?Sized> {
    span: Span<'de>,
    chunk: PhantomData<&'de T>,
}

impl<'de, T: ?Sized + Chunk> IntoIterator for Chunks<'de, T> {
    type Item = &'de T;
    type IntoIter = ChunkIter<'de, T>;

    fn into_iter(self) -> ChunkIter<'de, T> {
        self.span.chunks()
    }
}

impl<'de, T: ?Sized + Chunk> Iterator for ChunkIter<'de, T> {
    type Item = &'de T;

    fn next(&mut self) -> Option<&'de T> {
        let (chunk, rest) = self.span.split_first()?;
        self.span = rest;
        Some(unsafe { <T as sealed::Chunk>::assume(chunk) })
    }
}

impl<T: ?Sized> Clone for ChunkIter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            span: self.span,
            chunk: PhantomData,
        }
    }
}

impl<'de> Span<'de> {
    /// The first chunk, and the span after it.
    pub(super) fn split_first(self) -> Option<(&'de [u8], Self)> {
        use super::input::Input;
        use crate::wire::length;

        match self {
            Self::Definite(payload) => Some((payload, Self::Indefinite(&[]))),
            Self::Indefinite(items) => {
                let mut input = Input::new(items);
                input
                    .take_head()
                    .ok()
                    .flatten()
                    .and_then(|head| length(head.arg).ok())
                    .and_then(|len| input.take_exact(len))
                    .map(|chunk| (chunk, Self::Indefinite(input.rest())))
            }
        }
    }

    /// The bytes of chunks not yet yielded, heads included.
    pub(super) const fn unread(self) -> usize {
        match self {
            Self::Definite(payload) | Self::Indefinite(payload) => payload.len(),
        }
    }

    pub(super) const fn chunks<T: ?Sized>(self) -> ChunkIter<'de, T> {
        ChunkIter {
            span: self,
            chunk: PhantomData,
        }
    }
}

impl<T: ?Sized> Clone for Chunks<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Chunks<'_, T> {}

/// Equal when their strings are, however each was chunked.
impl<T: ?Sized> PartialEq for Chunks<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.span
            .chunks::<[u8]>()
            .flatten()
            .eq(other.span.chunks::<[u8]>().flatten())
    }
}

impl<T: ?Sized> Eq for Chunks<'_, T> {}

impl<T: ?Sized + Chunk + fmt::Debug> fmt::Debug for Chunks<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(*self).finish()
    }
}

unsafe impl<T: ?Sized + Chunk> Encode for Chunks<'_, T> {
    const ENCODE: &'static EncKind = <T as sealed::Chunk>::ENCODE;
    type Stack = Level<()>;
    const MAX_DEPTH: usize = 1;
}

unsafe impl<'de: 'a, 'a, T: ?Sized + Chunk> Decode<'de> for Chunks<'a, T> {
    const DECODE: &'static DecKind = &DecKind::Leaf {
        decode: decode::<T>,
        default: None,
        drop: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = <T as sealed::Chunk>::LEAD;
}

unsafe fn decode<T: ?Sized + Chunk>(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    of::<T>(item).map(|(chunks, next)| {
        unsafe { dst.cast::<Chunks<'_, T>>().write(chunks) };
        next
    })
}

/// The string `item` holds as chunks of `T`, and how the decoder goes on: an indefinite string
/// takes its chunks and break from the rest of the input, which must hold them.
///
/// # Errors
/// [`ErrorKind::TypeMismatch`] for another item; [`ErrorKind::InvalidUtf8`] for text that is not;
/// for chunks, any error skipping the string would.
pub(super) fn of<'a, T: ?Sized + Chunk>(
    item: &Item<'a>,
) -> Result<(Chunks<'a, T>, Next), ErrorKind> {
    use super::skip::rest_len;
    use super::text::validate;

    match (
        item.header.major == <T as sealed::Chunk>::MAJOR,
        item.header.indefinite(),
    ) {
        (false, _) => Err(ErrorKind::TypeMismatch),
        (true, false) => match item.header.major {
            Major::Text => validate(item.payload),
            _ => Ok(()),
        }
        .map(|()| (Span::Definite(item.payload), Next::Done)),
        (true, true) => rest_len(item.header, item.payload).map(|taken| {
            (
                Span::Indefinite(item.payload.get(..taken.saturating_sub(1)).unwrap_or(&[])),
                Next::Took(taken),
            )
        }),
    }
    .map(|(span, next)| {
        (
            Chunks {
                span,
                chunk: PhantomData,
            },
            next,
        )
    })
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::{Chunks, Span, of};
    use crate::engine::{Item, Next};
    use crate::error::ErrorKind;
    use crate::wire::{Header, Major};

    /// The chunks an indefinite text string's `rest` holds, and the bytes they and the break took.
    fn text(rest: &[u8]) -> Result<(Vec<&str>, usize), ErrorKind> {
        of::<str>(&Item {
            header: Header {
                major: Major::Text,
                info: 31,
                arg: 0,
            },
            payload: rest,
            strict: None,
        })
        .map(|(chunks, next)| {
            (
                chunks.into_iter().collect(),
                match next {
                    Next::Took(taken) => taken,
                    Next::Done | Next::Then(_) => 0,
                },
            )
        })
    }

    #[test]
    fn a_run_ends_at_its_break_and_holds_only_definite_chunks_of_its_type() {
        assert_eq!(text(b"\xff"), Ok((Vec::new(), 1)));
        assert_eq!(text(b"\x61a\x60\xff\x01"), Ok((Vec::from(["a", ""]), 4)));
        for (rest, error) in [
            (b"\x61a".as_slice(), ErrorKind::NeedContiguous),
            (b"\x62a", ErrorKind::NeedContiguous),
            (b"\x41a\xff", ErrorKind::NotWellFormed),
            (b"\x7f\xff\xff", ErrorKind::NotWellFormed),
            (b"\x62\xc3\x28\xff", ErrorKind::InvalidUtf8),
        ] {
            assert_eq!(text(rest), Err(error), "{rest:x?}");
        }
    }

    #[test]
    fn a_span_yields_each_chunk_empty_ones_included() {
        let chunks = |span: Span<'static>| span.chunks::<[u8]>().collect::<Vec<_>>();
        assert_eq!(chunks(Span::Definite(b"ab")), [b"ab"]);
        assert_eq!(chunks(Span::Definite(b"")), [b""]);
        assert_eq!(chunks(Span::Indefinite(b"")), [] as [&[u8]; 0]);
        assert_eq!(
            chunks(Span::Indefinite(b"\x41a\x40\x42bc")),
            [b"a".as_slice(), b"", b"bc"]
        );
        assert_eq!(
            Chunks::<str> {
                span: Span::Definite(b"abc"),
                chunk: core::marker::PhantomData
            },
            Chunks {
                span: Span::Indefinite(b"\x61a\x60\x62bc"),
                chunk: core::marker::PhantomData
            }
        );
    }
}
