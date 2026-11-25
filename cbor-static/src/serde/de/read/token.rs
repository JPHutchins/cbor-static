#![forbid(unsafe_code)]
//! The next token, and where an error reading it is reported.

use crate::ErrorKind;
use crate::serde::{Deserializer, Error};
use crate::token::Token;
use crate::wire::{INDEFINITE, Major, header_len};

/// The majors of the strings: a read neither satisfies refuses either at its head.
pub(super) const STRINGS: &[Major] = &[Major::Bytes, Major::Text];
/// The strings' majors and a map's: a scalar or an array refuses each at its head.
pub(super) const STRINGS_AND_MAPS: &[Major] = &[Major::Bytes, Major::Text, Major::Map];

impl<'de> Deserializer<'de> {
    /// The next token, and the offset of its head. A struct's key that is not text, which
    /// [`Self::next_key`] skipped, can be read only as an identifier or ignored. Under Strict, its
    /// head must be deterministic, and a key out of order is refused before its value.
    pub(super) fn token(&mut self) -> Result<(Token<'de>, usize), Error> {
        let at = self.ready()?;
        self.next_token(at)
    }

    /// [`Self::token`] for a read that a definite string of a major in `refused` never satisfies,
    /// which decides at the string's head as the native leaf does: once the head is whole, the
    /// string is a type mismatch just past it, whatever its payload holds and however much of it
    /// arrived. A head cut short is still the end of the input.
    pub(super) fn token_not(&mut self, refused: &[Major]) -> Result<(Token<'de>, usize), Error> {
        let at = self.ready()?;
        match self.tokens.initial() {
            Some(initial)
                if refused.contains(&Major::of(initial))
                    && initial & 0x1f != INDEFINITE
                    && header_len(initial).is_ok_and(|len| len <= self.tokens.rest().len()) =>
            {
                self.deterministic(at)?;
                Err(self.refused(at))
            }
            Some(_) | None => self.next_token(at),
        }
    }

    /// Where the next token starts, once a key out of order or skipped allows reading it.
    fn ready(&mut self) -> Result<usize, Error> {
        self.ordered()?;
        let at = self.tokens.offset();
        if core::mem::take(&mut self.skipped_key) {
            return Err(Error::new(ErrorKind::TypeMismatch, at));
        }
        Ok(at)
    }

    fn next_token(&mut self, at: usize) -> Result<(Token<'de>, usize), Error> {
        let end = at + self.tokens.rest().len();
        match self.tokens.next() {
            Some(Ok(token)) => self.deterministic(at).map(|()| (token, at)),
            Some(Err(error)) => Err(self.as_native(error.kind, at, end)),
            None => Err(Error::new(ErrorKind::UnexpectedEnd, at)),
        }
    }

    /// Under Strict, refuses the map key [`Self::next_key`] found out of order, once reading has
    /// passed it, at the offset just past it.
    pub(super) fn ordered(&mut self) -> Result<(), Error> {
        match self.unordered {
            Some(past_key) if self.tokens.offset() >= past_key => {
                self.unordered = None;
                Err(Error::new(ErrorKind::NotDeterministic, past_key))
            }
            Some(_) | None => Ok(()),
        }
    }

    /// Under Strict, refuses the head just read at `at` past it unless it is deterministic.
    fn deterministic(&self, at: usize) -> Result<(), Error> {
        match self.strict {
            Some(strict) => (strict.head)(self.tokens.span(at, self.tokens.head_end(at)))
                .map(|_| ())
                .map_err(|kind| self.past_head(kind, at)),
            None => Ok(()),
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
        if let Some(past_key) = self.unordered {
            return Err(Error::new(ErrorKind::NotDeterministic, past_key).into());
        }
        match self.tokens.rest() {
            [_, ..] if whole => {
                Err(Error::new(ErrorKind::TrailingInput, self.tokens.offset()).into())
            }
            rest => Ok(rest),
        }
    }
}
