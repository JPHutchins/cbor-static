use super::Token;
use super::window::{WHOLE_ITEM, Window};
use crate::engine::{Encoder, Progress};
use crate::error::{Error, ErrorKind};
use crate::schema::Encode;
use crate::wire::{FALSE, Head, Major, NULL, TRUE, UNDEFINED, float};

/// Writes tokens in preferred serialization (RFC 8949 §4.1), with definite lengths only (D-11).
///
/// ```
/// use cbor_static::{Token, TokenWriter};
///
/// let mut out = [0u8; 8];
/// let mut writer = TokenWriter::new(&mut out);
/// writer.write(Token::Array(Some(2)))?.write(Token::Float(1.5))?.write(Token::Text("a"))?;
/// let len = writer.written();
/// assert_eq!(&out[..len], b"\x82\xf9\x3e\x00\x61a");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug)]
pub struct TokenWriter<'a> {
    pub(super) out: &'a mut [u8],
    pub(super) at: usize,
    pub(super) item: Option<&'a mut Window>,
}

impl<'a> TokenWriter<'a> {
    /// ```
    /// assert_eq!(cbor_static::TokenWriter::new(&mut [0u8; 4]).written(), 0);
    /// ```
    #[must_use]
    pub const fn new(out: &'a mut [u8]) -> Self {
        Self {
            out,
            at: 0,
            item: None,
        }
    }

    /// A writer of exactly one item, of which `out` takes the part `window` covers.
    pub(crate) const fn window(out: &'a mut [u8], window: &'a mut Window) -> Self {
        Self {
            out,
            at: 0,
            item: Some(window),
        }
    }

    /// The length of the one item a [`TokenWriter::window`] wrote.
    ///
    /// # Errors
    /// [`ErrorKind::NotWellFormed`] when it wrote none, or left one unfinished.
    pub(crate) fn item_len(&self) -> Result<usize, ErrorKind> {
        match self.item.as_deref() {
            Some(window) if window.done() => Ok(self.at),
            Some(_) | None => Err(ErrorKind::NotWellFormed),
        }
    }

    /// Appends `token`, or nothing when it fails.
    ///
    /// ```
    /// use cbor_static::{ErrorKind, Token, TokenWriter};
    ///
    /// let mut out = [0u8; 2];
    /// let mut writer = TokenWriter::new(&mut out);
    /// assert_eq!(writer.write(Token::Unsigned(500)).map_err(|e| e.kind).err(), Some(ErrorKind::BufferTooSmall));
    /// assert_eq!(writer.write(Token::Array(None)).map_err(|e| e.kind).err(), Some(ErrorKind::Unsupported));
    /// assert_eq!(writer.write(Token::Simple(22)).map_err(|e| e.kind).err(), Some(ErrorKind::Unsupported));
    /// assert_eq!(writer.write(Token::Simple(24)).map_err(|e| e.kind).err(), Some(ErrorKind::NotWellFormed));
    /// assert_eq!(writer.write(Token::Simple(32))?.written(), 2);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    /// - [`ErrorKind::BufferTooSmall`] when the rest of the buffer cannot hold it;
    /// - [`ErrorKind::Unsupported`] for an indefinite length, a break, or a simple value with a
    ///   token of its own (20 to 23), so every token written reads back as itself;
    /// - [`ErrorKind::NotWellFormed`] for the simple values 24 to 31, which have no encoding.
    #[inline(never)]
    pub fn write(&mut self, token: Token<'_>) -> Result<&mut Self, Error> {
        if let Some(write) = self.item.as_deref().map(|window| window.ops.write) {
            return write(self, token);
        }
        let (head, payload) = encoding(token).map_err(|kind| Error::new(kind, 0, self.at))?;
        let len = head.as_bytes().len() + payload.len();
        self.out
            .get_mut(self.at..)
            .and_then(|rest| rest.get_mut(..len))
            .ok_or(Error::new(ErrorKind::BufferTooSmall, 0, self.at))?
            .iter_mut()
            .zip(head.as_bytes().iter().chain(payload))
            .for_each(|(slot, byte)| *slot = *byte);
        self.at += len;
        Ok(self)
    }

    /// Appends `value`'s encoding (#5 Q11), or when it fails, leaves [`TokenWriter::written`] as it
    /// was.
    ///
    /// ```
    /// use cbor_static::{ErrorKind, Token, TokenWriter};
    ///
    /// let mut out = [0u8; 6];
    /// let mut writer = TokenWriter::new(&mut out);
    /// writer.write(Token::Array(Some(2)))?.encode(&(1u8, "a"))?;
    /// assert_eq!(
    ///     writer.encode(&500u16).map_err(|e| (e.kind, e.offset)).err(),
    ///     Some((ErrorKind::BufferTooSmall, 5))
    /// );
    /// assert_eq!(writer.encode(&true)?.written(), 6);
    /// assert_eq!(out, *b"\x82\x82\x01\x61a\xf5");
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`ErrorKind::BufferTooSmall`] when the rest of the buffer cannot hold it, and any error
    /// encoding it, as [`crate::to_slice`] gives, at offsets into the buffer.
    #[inline(never)]
    pub fn encode<T: Encode>(&mut self, value: &T) -> Result<&mut Self, Error> {
        let at = self.at;
        match self.item.as_deref_mut() {
            None => match Encoder::new(value)
                .resume(self.out.get_mut(at..).unwrap_or(&mut []))
                .map_err(|error| error.after(at))?
            {
                Progress::Done(written) => self.at += written,
                Progress::NeedSpace(_) => {
                    return Err(Error::new(ErrorKind::BufferTooSmall, 0, at));
                }
            },
            Some(window) => {
                let before = *window;
                self.at =
                    streamed(value, window, self.out, at).inspect_err(|_| *window = before)?;
            }
        }
        Ok(self)
    }

    /// ```
    /// use cbor_static::{Token, TokenWriter};
    ///
    /// let mut out = [0u8; 4];
    /// assert_eq!(TokenWriter::new(&mut out).write(Token::Text("abc"))?.written(), 4);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn written(&self) -> usize {
        self.at
    }
}

/// Encodes `value` at offset `at` of a window's item, through a small buffer on the stack, keeping
/// the bytes `window` covers in `out`, and returns the offset after it.
fn streamed<T: Encode>(
    value: &T,
    window: &mut Window,
    out: &mut [u8],
    at: usize,
) -> Result<usize, Error> {
    (window.ops.owe)(window, WHOLE_ITEM).map_err(|kind| Error::new(kind, 0, at))?;
    let mut encoder = Encoder::new(value);
    let mut scratch = [0u8; 64];
    let mut offset = at;
    loop {
        let (written, done) = match encoder
            .resume(&mut scratch)
            .map_err(|error| error.after(at))?
        {
            Progress::NeedSpace(0) => return Err(Error::new(ErrorKind::BufferTooSmall, 0, at)),
            Progress::NeedSpace(written) => (written, false),
            Progress::Done(written) => (written, true),
        };
        (window.ops.put)(window, out, offset, scratch.get(..written).unwrap_or(&[]));
        offset += written;
        if done {
            return Ok(offset);
        }
    }
}

pub(super) fn encoding(token: Token<'_>) -> Result<(Head, &[u8]), ErrorKind> {
    match token {
        Token::Unsigned(n) => Ok((Head::new(Major::Uint, n), &[])),
        Token::Negative(n) => Ok((Head::new(Major::Nint, n), &[])),
        Token::Bytes(bytes) => Ok((Head::new(Major::Bytes, bytes.len() as u64), bytes)),
        Token::Text(text) => Ok((Head::new(Major::Text, text.len() as u64), text.as_bytes())),
        Token::Array(Some(n)) => Ok((Head::new(Major::Array, n), &[])),
        Token::Map(Some(n)) => Ok((Head::new(Major::Map, n), &[])),
        Token::Tag(n) => Ok((Head::new(Major::Tag, n), &[])),
        Token::Simple(value @ (0..FALSE | 32..)) => {
            Ok((Head::new(Major::Simple, value.into()), &[]))
        }
        Token::Simple(24..32) => Err(ErrorKind::NotWellFormed),
        Token::Bool(value) => Ok((
            Head::new(Major::Simple, if value { TRUE } else { FALSE }.into()),
            &[],
        )),
        Token::Null => Ok((Head::new(Major::Simple, NULL.into()), &[])),
        Token::Undefined => Ok((Head::new(Major::Simple, UNDEFINED.into()), &[])),
        Token::Float(value) => Ok((Head::float(float::shortest(value.to_bits())), &[])),
        Token::Simple(FALSE..24)
        | Token::BytesIndefinite
        | Token::TextIndefinite
        | Token::Array(None)
        | Token::Map(None)
        | Token::Break => Err(ErrorKind::Unsupported),
    }
}
