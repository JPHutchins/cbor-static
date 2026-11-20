#![forbid(unsafe_code)]
//! Strings read whole or chunk by chunk: a `char` and a bignum's digits into fixed state, as the
//! native decoder reads them, and with `alloc`, any other string assembled (D-13a).

#[cfg(feature = "alloc")]
use super::any::Value;
use crate::ErrorKind;
use crate::serde::{Deserializer, Error};
use crate::token::Token;

/// A `char`'s UTF-8 as its chunks arrive: its first four bytes, and how many there were.
#[derive(Clone, Copy, Default)]
pub(super) struct Utf8 {
    bytes: [u8; 4],
    len: usize,
}

/// A bignum's magnitude as its digits arrive, how many of them are past its leading zeros, and how
/// many there are in all.
#[derive(Clone, Copy, Default)]
pub(super) struct Digits {
    magnitude: u128,
    significant: usize,
    len: usize,
}

impl<'de> Deserializer<'de> {
    /// Folds `step` over the string `head` began: itself when definite, or each of its chunks up
    /// to its break. A step's error is where reading has reached, past the piece it refused.
    pub(super) fn fold_string<S>(
        &mut self,
        head: Token<'de>,
        mut state: S,
        step: fn(S, &[u8]) -> Result<S, ErrorKind>,
    ) -> Result<S, Error> {
        match head {
            Token::Text(text) => step(state, text.as_bytes()).map_err(|kind| self.here(kind)),
            Token::Bytes(bytes) => step(state, bytes).map_err(|kind| self.here(kind)),
            _ => {
                while let Some(chunk) = self.chunk()? {
                    state = step(state, chunk).map_err(|kind| self.here(kind))?;
                }
                Ok(state)
            }
        }
    }

    /// The next chunk of an indefinite string, or `None` at its break.
    #[inline(never)]
    fn chunk(&mut self) -> Result<Option<&'de [u8]>, Error> {
        match self.token().map_err(|error| self.copied(error))? {
            (Token::Text(text), _) => Ok(Some(text.as_bytes())),
            (Token::Bytes(bytes), _) => Ok(Some(bytes)),
            (Token::Break, _) => Ok(None),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// The chunks of an indefinite text string, up to its break, in one string.
    #[cfg(feature = "alloc")]
    pub(super) fn assembled_text(&mut self) -> Result<Value<'de, str>, Error> {
        alloc::string::String::from_utf8(self.assembled()?)
            .map(alloc::borrow::Cow::Owned)
            .map_err(|_| self.here(ErrorKind::InvalidUtf8))
    }

    /// The chunks of an indefinite string, up to its break, in one vector of exactly their length,
    /// reserved once.
    #[cfg(feature = "alloc")]
    pub(super) fn assembled(&mut self) -> Result<alloc::vec::Vec<u8>, Error> {
        let mut whole = alloc::vec::Vec::new();
        whole
            .try_reserve_exact(
                self.tokens
                    .clone()
                    .map_while(|token| match token {
                        Ok(Token::Text(text)) => Some(text.len()),
                        Ok(Token::Bytes(bytes)) => Some(bytes.len()),
                        _ => None,
                    })
                    .sum(),
            )
            .map_err(|_| self.here(ErrorKind::OutOfMemory))?;
        while let Some(chunk) = self.chunk()? {
            crate::engine::zeroed(&mut whole, chunk.len())
                .map_err(|kind| self.here(kind))?
                .iter_mut()
                .zip(chunk)
                .for_each(|(slot, byte)| *slot = *byte);
        }
        Ok(whole)
    }
}

impl Utf8 {
    /// Takes the next piece of a character's UTF-8, keeping its first four bytes.
    pub(super) fn step(self, piece: &[u8]) -> Self {
        let mut bytes = self.bytes;
        bytes
            .iter_mut()
            .skip(self.len)
            .zip(piece)
            .for_each(|(slot, byte)| *slot = *byte);
        Self {
            bytes,
            len: self.len.saturating_add(piece.len()),
        }
    }

    /// The one character these bytes are.
    pub(super) fn only(self) -> Option<char> {
        let mut chars = core::str::from_utf8(self.bytes.get(..self.len)?)
            .ok()?
            .chars();
        match (chars.next(), chars.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    }
}

impl Digits {
    /// Takes the next piece of a bignum's digits, of which at most 16 follow its leading zeros.
    pub(super) fn step(self, piece: &[u8]) -> Result<Self, ErrorKind> {
        piece
            .iter()
            .try_fold(self, |digits, &digit| match digits {
                Self { significant: 0, .. } if digit == 0 => Ok(digits),
                Self {
                    significant: 16.., ..
                } => Err(ErrorKind::OutOfRange),
                Self {
                    magnitude,
                    significant,
                    ..
                } => Ok(Self {
                    magnitude: magnitude << 8 | u128::from(digit),
                    significant: significant + 1,
                    ..digits
                }),
            })
            .map(|digits| Self {
                len: digits.len + piece.len(),
                ..digits
            })
    }

    pub(super) const fn magnitude(self) -> u128 {
        self.magnitude
    }

    pub(super) const fn len(self) -> usize {
        self.len
    }
}
