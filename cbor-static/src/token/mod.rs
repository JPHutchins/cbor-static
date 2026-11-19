#![forbid(unsafe_code)]
//! CBOR as a flat stream of heads, each with its definite string's payload (D-20), over
//! contiguous input, and typed items handed off in the middle of one (#5 Q11).

mod window;
mod writer;

use core::iter::FusedIterator;
use core::mem::MaybeUninit;

pub(crate) use window::Window;
pub use writer::TokenWriter;

use crate::engine::{Decoder, Fed, Input, Skip, StrictOps, utf8};
use crate::error::{Error, ErrorKind};
use crate::schema::Decode;
use crate::wire::{FALSE, Header, INDEFINITE, Major, NULL, TRUE, UNDEFINED, float, length};

/// One head, with the payload of a definite string. `Negative(n)` is `-1 - n`, and `None` is an
/// indefinite length. Floats compare by their bits (D-22a).
///
/// ```
/// use cbor_static::{Token, Tokens};
///
/// let tokens: Vec<Token> = Tokens::new(b"\x83\x20\xf9\x3e\x00\x7f\x61a\xff").collect::<Result<_, _>>()?;
/// assert_eq!(
///     tokens,
///     [
///         Token::Array(Some(3)),
///         Token::Negative(0),
///         Token::Float(1.5),
///         Token::TextIndefinite,
///         Token::Text("a"),
///         Token::Break,
///     ]
/// );
/// assert_eq!(Token::Float(f64::NAN), Token::Float(f64::NAN));
/// assert_ne!(Token::Float(0.0), Token::Float(-0.0));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub enum Token<'de> {
    Unsigned(u64),
    Negative(u64),
    Bytes(&'de [u8]),
    Text(&'de str),
    BytesIndefinite,
    TextIndefinite,
    Array(Option<u64>),
    Map(Option<u64>),
    Tag(u64),
    Simple(u8),
    Bool(bool),
    Null,
    Undefined,
    Float(f64),
    Break,
}

impl PartialEq for Token<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Unsigned(a), Self::Unsigned(b))
            | (Self::Negative(a), Self::Negative(b))
            | (Self::Tag(a), Self::Tag(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Array(a), Self::Array(b)) | (Self::Map(a), Self::Map(b)) => a == b,
            (Self::Simple(a), Self::Simple(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::BytesIndefinite, Self::BytesIndefinite)
            | (Self::TextIndefinite, Self::TextIndefinite)
            | (Self::Null, Self::Null)
            | (Self::Undefined, Self::Undefined)
            | (Self::Break, Self::Break) => true,
            _ => false,
        }
    }
}

impl Eq for Token<'_> {}

/// The tokens of a CBOR sequence (RFC 8742), checked for well-formedness (RFC 8949 Appendix F)
/// with the iterative skip state (D-14a, D-14b). An error ends the iteration. Nesting past the
/// state's resource limits (RFC 8949 §5.1) is `LimitExceeded`: more than 64 open indefinite
/// levels, or more than 8 indefinite items opened while their parent still owes items, which
/// includes an indefinite key of an indefinite map.
///
/// ```
/// use cbor_static::{ErrorKind, Token, Tokens};
///
/// let mut tokens = Tokens::new(b"\x01\x9f\xff\x82\x01");
/// assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(1))));
/// assert_eq!(tokens.next(), Some(Ok(Token::Array(None))));
/// assert_eq!(tokens.next(), Some(Ok(Token::Break)));
/// assert_eq!(tokens.next(), Some(Ok(Token::Array(Some(2)))));
/// assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(1))));
/// assert_eq!(tokens.next().map(|t| t.map_err(|e| (e.kind, e.offset))), Some(Err((ErrorKind::UnexpectedEnd, 5))));
/// assert_eq!(tokens.next(), None);
///
/// let at_limit = [[0x9f; 64], [0xff; 64]].concat();
/// assert!(Tokens::new(&at_limit).all(|token| token.is_ok()));
/// let past = [[0x9f; 65], [0xff; 65]].concat();
/// assert_eq!(Tokens::new(&past).find_map(Result::err).map(|e| e.kind), Some(ErrorKind::LimitExceeded));
/// ```
#[derive(Clone, Debug)]
pub struct Tokens<'de> {
    input: Input<'de>,
    skip: Skip,
    strict: Option<&'static StrictOps>,
}

/// The head of an item taken whole, as the skip state counts it: one item, opening nothing.
const WHOLE_ITEM: Header = Header {
    major: Major::Uint,
    info: 0,
    arg: 0,
};

impl<'de> Tokens<'de> {
    /// ```
    /// assert_eq!(cbor_static::Tokens::new(b"").next(), None);
    /// ```
    #[must_use]
    pub const fn new(bytes: &'de [u8]) -> Self {
        Self::under(bytes, None)
    }

    /// The tokens of `bytes`, whose typed items [`Tokens::decode_into`] decodes under the profile
    /// `strict` names: a `with` field's module reads its item through these (#5 Q19).
    pub(crate) const fn under(bytes: &'de [u8], strict: Option<&'static StrictOps>) -> Self {
        Self {
            input: Input::new(bytes),
            skip: Skip::started(0),
            strict,
        }
    }

    /// Decodes the next item as `T` into `out` (D-15), and moves past it. On failure the tokens stay
    /// where they were, so the item can be decoded as another type, or read token by token.
    ///
    /// ```
    /// use core::mem::MaybeUninit;
    /// use cbor_static::{ErrorKind, Token, Tokens};
    ///
    /// #[derive(cbor_static::Decode, Debug, PartialEq)]
    /// struct Point {
    ///     x: u32,
    /// }
    ///
    /// let mut tokens = Tokens::new(b"\x82\xa1\x61x\x01\x07");
    /// assert_eq!(tokens.next(), Some(Ok(Token::Array(Some(2)))));
    /// assert_eq!(*tokens.decode_into::<Point>(&mut MaybeUninit::uninit())?, Point { x: 1 });
    /// assert_eq!(
    ///     tokens.decode_into::<Point>(&mut MaybeUninit::uninit()).map_err(|e| (e.kind, e.offset)),
    ///     Err((ErrorKind::TypeMismatch, 6))
    /// );
    /// assert_eq!(*tokens.decode_into::<u8>(&mut MaybeUninit::uninit())?, 7);
    /// assert_eq!(tokens.next(), None);
    ///
    /// let mut chunks = Tokens::new(b"\x5f\x41\x01\xff");
    /// assert_eq!(chunks.next(), Some(Ok(Token::BytesIndefinite)));
    /// assert_eq!(
    ///     chunks.decode_into::<u8>(&mut MaybeUninit::uninit()).map_err(|e| (e.kind, e.offset)),
    ///     Err((ErrorKind::NotWellFormed, 1))
    /// );
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    /// Any of [`crate::decode_into`]'s but [`ErrorKind::TrailingInput`], at offsets into these
    /// tokens' input, and [`ErrorKind::NotWellFormed`] where no item may begin: inside an
    /// indefinite-length string.
    #[inline(never)]
    pub fn decode_into<'s, T: Decode<'de>>(
        &mut self,
        out: &'s mut MaybeUninit<T>,
    ) -> Result<&'s mut T, Error> {
        let at = self.input.at;
        let mut skip = self.skip;
        skip.on_header(WHOLE_ITEM)
            .map_err(|kind| Error::new(kind, 0, at))?;
        let mut decoder = Decoder::under(out, self.strict);
        match decoder.feed(self.input.rest()) {
            Ok(Fed::Done(used)) => {
                self.input.at = at + used;
                self.skip = skip;
                decoder.finish()
            }
            Ok(Fed::NeedInput) => Err(Error::new(
                ErrorKind::UnexpectedEnd,
                0,
                at + self.input.rest().len(),
            )),
            Err(error) => Err(Error {
                kind: match error.kind {
                    ErrorKind::NeedContiguous => ErrorKind::UnexpectedEnd,
                    kind => kind,
                },
                ..error.after(at)
            }),
        }
    }

    fn token(&mut self) -> Result<Token<'de>, ErrorKind> {
        let header = self.input.take_head()?.ok_or(ErrorKind::UnexpectedEnd)?;
        self.skip.on_header(header)?;
        match (header.major, header.indefinite()) {
            (Major::Uint, _) => Ok(Token::Unsigned(header.arg)),
            (Major::Nint, _) => Ok(Token::Negative(header.arg)),
            (Major::Bytes, false) => self.payload(header.arg).map(Token::Bytes),
            (Major::Text, false) => self.payload(header.arg).and_then(utf8).map(Token::Text),
            (Major::Bytes, true) => Ok(Token::BytesIndefinite),
            (Major::Text, true) => Ok(Token::TextIndefinite),
            (Major::Array, indefinite) => Ok(Token::Array((!indefinite).then_some(header.arg))),
            (Major::Map, indefinite) => Ok(Token::Map((!indefinite).then_some(header.arg))),
            (Major::Tag, _) => Ok(Token::Tag(header.arg)),
            (Major::Simple, _) => simple(header),
        }
    }

    fn payload(&mut self, len: u64) -> Result<&'de [u8], ErrorKind> {
        let payload = self
            .input
            .take_exact(length(len)?)
            .ok_or(ErrorKind::UnexpectedEnd)?;
        self.skip.took_payload();
        Ok(payload)
    }
}

impl<'de> Iterator for Tokens<'de> {
    type Item = Result<Token<'de>, Error>;

    #[inline(never)]
    fn next(&mut self) -> Option<Self::Item> {
        let at = self.input.at;
        if self.input.rest().is_empty() && self.skip.done() {
            return None;
        }
        Some(self.token().map_err(|kind| {
            self.input.take_rest();
            self.skip = Skip::started(0);
            Error::new(kind, 0, at)
        }))
    }
}

impl FusedIterator for Tokens<'_> {}

fn simple(header: Header) -> Result<Token<'static>, ErrorKind> {
    match header.info {
        FALSE => Ok(Token::Bool(false)),
        TRUE => Ok(Token::Bool(true)),
        NULL => Ok(Token::Null),
        UNDEFINED => Ok(Token::Undefined),
        INDEFINITE => Ok(Token::Break),
        info => float::value(info, header.arg).map_or_else(
            || {
                u8::try_from(header.arg)
                    .map(Token::Simple)
                    .map_err(|_| ErrorKind::NotWellFormed)
            },
            |bits| Ok(Token::Float(f64::from_bits(bits))),
        ),
    }
}

#[cfg(test)]
mod tests;
