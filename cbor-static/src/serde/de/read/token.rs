#![forbid(unsafe_code)]
//! The next token, and where an error reading it is reported.

use crate::ErrorKind;
use crate::serde::{Deserializer, Error};
use crate::token::Token;

impl<'de> Deserializer<'de> {
    /// The next token, and the offset of its head. A struct's key that is not text, which
    /// [`Self::next_key`] skipped, can be read only as an identifier or ignored.
    pub(super) fn token(&mut self) -> Result<(Token<'de>, usize), Error> {
        let at = self.tokens.offset();
        if core::mem::take(&mut self.skipped_key) {
            return Err(Error::new(ErrorKind::TypeMismatch, at));
        }
        let end = at + self.tokens.rest().len();
        match self.tokens.next() {
            Some(Ok(token)) => Ok((token, at)),
            Some(Err(error)) => Err(self.as_native(error.kind, at, end)),
            None => Err(Error::new(ErrorKind::UnexpectedEnd, at)),
        }
    }

    /// `kind`, which the tokens gave for the head at `at` of input ending at `end`, where the
    /// native decoder reports it: at the end for a head cut short, past the payload for text that
    /// is not UTF-8, and past the head otherwise.
    fn as_native(&self, kind: ErrorKind, at: usize, end: usize) -> Error {
        let head_end = self.tokens.head_end(at);
        Error::new(
            kind,
            match kind {
                ErrorKind::UnexpectedEnd if head_end > end => end,
                ErrorKind::InvalidUtf8 => self.tokens.payload_end(at),
                _ => head_end,
            },
        )
    }

    /// `kind` just past the head at `at`, where the native decoder reports a refused item.
    pub(super) fn past_head(&self, kind: ErrorKind, at: usize) -> Error {
        Error::new(kind, self.tokens.head_end(at))
    }

    /// A type mismatch just past the head at `at`, as natively, which also refuses a tag there.
    pub(super) fn refused(&self, at: usize) -> Error {
        self.past_head(ErrorKind::TypeMismatch, at)
    }

    /// `error` from reading a string the native decoder copies rather than borrows (a key, a name,
    /// a character, a bignum's digits): one cut short is reported at the end of the input.
    pub(super) fn copied(&self, error: Error) -> Error {
        match error.kind {
            ErrorKind::UnexpectedEnd => self.here(error.kind),
            _ => error,
        }
    }

    /// `kind` where reading has reached: past a string, which the native decoder also takes whole
    /// before it refuses it as the other kind of string, or as a character.
    pub(super) fn here(&self, kind: ErrorKind) -> Error {
        Error::new(kind, self.tokens.offset())
    }

    /// The input after an item decoded, which must be empty when the item was to span it `whole`.
    #[inline(never)]
    pub(in crate::serde::de) fn rest(&self, whole: bool) -> Result<&'de [u8], crate::Error> {
        match self.tokens.rest() {
            [_, ..] if whole => {
                Err(Error::new(ErrorKind::TrailingInput, self.tokens.offset()).into())
            }
            rest => Ok(rest),
        }
    }
}
