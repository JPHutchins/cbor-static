#![forbid(unsafe_code)]
//! CBOR as a flat stream of heads, each with its definite string's payload (D-20), over
//! contiguous input. Typed decoding in the middle of a stream is Phase 2.

mod writer;

use core::iter::FusedIterator;

pub use writer::TokenWriter;

use crate::engine::{Skip, utf8};
use crate::error::{Error, ErrorKind};
use crate::wire::{
    FALSE, Header, INDEFINITE, Major, NULL, TRUE, UNDEFINED, float, header_len, length, parse,
};

/// One head, with the payload of a definite string. `Negative(n)` is `-1 - n`, and `None` is an
/// indefinite length.
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
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// The tokens of a CBOR sequence (RFC 8742), checked for well-formedness (RFC 8949 Appendix F)
/// with the iterative skip state (D-14a). An error ends the iteration.
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
/// ```
#[derive(Clone, Debug)]
pub struct Tokens<'de> {
    bytes: &'de [u8],
    at: usize,
    skip: Skip,
}

impl<'de> Tokens<'de> {
    /// ```
    /// assert_eq!(cbor_static::Tokens::new(b"").next(), None);
    /// ```
    #[must_use]
    pub const fn new(bytes: &'de [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            skip: Skip::started(0),
        }
    }

    fn token(&mut self) -> Result<Token<'de>, ErrorKind> {
        let rest = self.bytes.get(self.at..).unwrap_or(&[]);
        let head = rest
            .get(..header_len(*rest.first().ok_or(ErrorKind::UnexpectedEnd)?)?)
            .ok_or(ErrorKind::UnexpectedEnd)?;
        let header = parse(head)?;
        self.skip.on_header(header)?;
        self.at += head.len();
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
        let payload = length(len).and_then(|len| {
            self.bytes
                .get(self.at..)
                .and_then(|rest| rest.get(..len))
                .ok_or(ErrorKind::UnexpectedEnd)
        })?;
        self.skip.took_payload();
        self.at += payload.len();
        Ok(payload)
    }
}

impl<'de> Iterator for Tokens<'de> {
    type Item = Result<Token<'de>, Error>;

    #[inline(never)]
    fn next(&mut self) -> Option<Self::Item> {
        let at = self.at;
        if at >= self.bytes.len() && self.skip.done() {
            return None;
        }
        Some(self.token().map_err(|kind| {
            self.at = self.bytes.len();
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
            |value| Ok(Token::Float(value)),
        ),
    }
}

#[cfg(test)]
mod tests;
