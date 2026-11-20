#![forbid(unsafe_code)]
//! The reads of one item's value.

use crate::ErrorKind;
use crate::serde::{Deserializer, Error};
use crate::token::Token;

use super::any::Value;
use super::strings::{Digits, Utf8};

impl<'de> Deserializer<'de> {
    #[inline(never)]
    pub(in crate::serde::de) fn boolean(&mut self) -> Result<bool, Error> {
        match self.token()? {
            (Token::Bool(value), _) => Ok(value),
            (_, at) => Err(self.refused(at)),
        }
    }

    #[inline(never)]
    pub(in crate::serde::de) fn null(&mut self) -> Result<(), Error> {
        match self.token()? {
            (Token::Null, _) => Ok(()),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// Moves past the next item, or past nothing when it is a struct's key that is not text,
    /// which [`Self::next_key`] skipped already.
    #[inline(never)]
    pub(in crate::serde::de) fn ignored(&mut self) -> Result<(), Error> {
        if core::mem::take(&mut self.skipped_key) {
            return Ok(());
        }
        Ok(self.tokens.skip_item()?)
    }

    /// Whether the next item is null, which it then reads.
    #[inline(never)]
    pub(in crate::serde::de) fn took_null(&mut self) -> Result<bool, Error> {
        use super::NULL;

        match self.tokens.initial() {
            Some(NULL) => self.null().map(|()| true),
            _ => Ok(false),
        }
    }

    /// An integer as `N`, which floats are not (D-12).
    #[inline(never)]
    pub(in crate::serde::de) fn integer<N: TryFrom<i128>>(&mut self) -> Result<N, Error> {
        let (token, at) = self.token()?;
        match token {
            Token::Unsigned(n) => Ok(i128::from(n)),
            Token::Negative(n) => Ok(!i128::from(n)),
            _ => Err(self.refused(at)),
        }
        .and_then(|n| N::try_from(n).map_err(|_| self.past_head(ErrorKind::OutOfRange, at)))
    }

    /// A `u128`: an integer, or a bignum of tag 2 (D-12).
    #[inline(never)]
    pub(in crate::serde::de) fn wide_unsigned(&mut self) -> Result<u128, Error> {
        match self.token()? {
            (Token::Unsigned(n), _) => Ok(n.into()),
            (Token::Tag(2), _) => self.digits(),
            (Token::Negative(_) | Token::Tag(3), at) => {
                Err(self.past_head(ErrorKind::OutOfRange, at))
            }
            (_, at) => Err(self.refused(at)),
        }
    }

    /// An `i128`: an integer, or a bignum of tag 2 or 3 (D-12).
    #[inline(never)]
    pub(in crate::serde::de) fn wide_signed(&mut self) -> Result<i128, Error> {
        match self.token()? {
            (Token::Unsigned(n), _) => Ok(n.into()),
            (Token::Negative(n), _) => Ok(!i128::from(n)),
            (Token::Tag(2), _) => {
                i128::try_from(self.digits()?).map_err(|_| self.here(ErrorKind::OutOfRange))
            }
            (Token::Tag(3), _) => i128::try_from(self.digits()?)
                .map(|magnitude| !magnitude)
                .map_err(|_| self.here(ErrorKind::OutOfRange)),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// A bignum's magnitude: a byte string, whole or in chunks, of at most 16 digits past its
    /// leading zeros.
    pub(super) fn digits(&mut self) -> Result<u128, Error> {
        match self.token().map_err(|error| self.copied(error))? {
            (head @ (Token::Bytes(_) | Token::BytesIndefinite), _) => self
                .fold_string(head, Digits::default(), Digits::step)
                .map(Digits::magnitude),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// A float's binary64 bits, which integers are not (D-12).
    #[inline(never)]
    pub(in crate::serde::de) fn double(&mut self) -> Result<u64, Error> {
        match self.token()? {
            (Token::Float(value), _) => Ok(value.to_bits()),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// A float's binary32 bits, when it narrows exactly (D-12).
    #[inline(never)]
    pub(in crate::serde::de) fn single(&mut self) -> Result<u32, Error> {
        let at = self.tokens.offset();
        crate::wire::float::to_single(self.double()?)
            .ok_or(self.past_head(ErrorKind::OutOfRange, at))
    }

    /// Text, borrowed, or with `alloc` assembled from chunks (D-13a).
    #[inline(never)]
    pub(in crate::serde::de) fn text(&mut self) -> Result<Value<'de, str>, Error> {
        match self.token()? {
            (Token::Text(text), _) => Ok(text.into()),
            #[cfg(feature = "alloc")]
            (Token::TextIndefinite, _) => self.assembled_text(),
            #[cfg(not(feature = "alloc"))]
            (Token::TextIndefinite, at) => Err(self.past_head(ErrorKind::IndefiniteString, at)),
            (Token::Bytes(_), _) => Err(self.here(ErrorKind::TypeMismatch)),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// Bytes, borrowed, or with `alloc` assembled from chunks (D-13a).
    #[inline(never)]
    pub(in crate::serde::de) fn bytes(&mut self) -> Result<Value<'de, [u8]>, Error> {
        match self.token()? {
            (Token::Bytes(bytes), _) => Ok(bytes.into()),
            #[cfg(feature = "alloc")]
            (Token::BytesIndefinite, _) => self.assembled().map(alloc::borrow::Cow::Owned),
            #[cfg(not(feature = "alloc"))]
            (Token::BytesIndefinite, at) => Err(self.past_head(ErrorKind::IndefiniteString, at)),
            (Token::Text(_), _) => Err(self.here(ErrorKind::TypeMismatch)),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// Text, whole or in chunks, of exactly one character.
    #[inline(never)]
    pub(in crate::serde::de) fn character(&mut self) -> Result<char, Error> {
        match self.token().map_err(|error| self.copied(error))? {
            (head @ (Token::Text(_) | Token::TextIndefinite), _) => self
                .fold_string(head, Utf8::default(), |utf8, piece| Ok(utf8.step(piece)))?
                .only()
                .ok_or_else(|| self.here(ErrorKind::TypeMismatch)),
            (Token::Bytes(_), _) => Err(self.here(ErrorKind::TypeMismatch)),
            (_, at) => Err(self.refused(at)),
        }
    }
}
