#![forbid(unsafe_code)]
//! The reads of arrays, maps and variants.

use super::{Entries, Keys, Remaining};
use crate::ErrorKind;
use crate::serde::{Deserializer, Error};
use crate::token::Token;
use crate::wire::Major;

impl<'de> Deserializer<'de> {
    /// A struct's key or a variant's name, or `None` in place of a struct's key that is not text,
    /// which [`Self::next_key`] skipped.
    #[inline(never)]
    pub(in crate::serde::de) fn identifier(&mut self) -> Result<Option<&'de str>, Error> {
        if core::mem::take(&mut self.skipped_key) {
            return Ok(None);
        }
        match self.token().map_err(|error| self.copied(error))? {
            (Token::Text(name), _) => Ok(Some(name)),
            (Token::TextIndefinite, at) => Err(self.past_head(ErrorKind::Unsupported, at)),
            (Token::Bytes(_), _) => Err(self.here(ErrorKind::TypeMismatch)),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// Whether another entry of the map `keys` reads follows. Before a struct's key, one that is not
    /// text is skipped, so [`Self::identifier`] names no field in its place (D-12). Under Strict, a
    /// key that does not sort after the key before it is refused once its value is read: an
    /// unknown field or a duplicate is refused as such first, as the native Strict decoder
    /// refuses them. A key not text is checked whole before it is skipped, and refused past its
    /// first head.
    #[inline(never)]
    pub(in crate::serde::de) fn next_key(&mut self, keys: &mut Keys<'de>) -> Result<bool, Error> {
        if !self.more(&mut keys.remaining)? {
            return Ok(false);
        }
        let at = self.tokens.offset();
        if let Some(strict) = self.strict {
            let key = (strict.order)(keys.previous, self.tokens.rest());
            self.unordered = self
                .unordered
                .or((!key.rises).then_some(at + key.encoding.len()));
            keys.previous = key.encoding;
        }
        match (keys.entries, self.tokens.initial().map(Major::of)) {
            (Entries::Map, _) | (Entries::Struct, Some(Major::Text) | None) => {}
            (Entries::Struct, Some(_)) => {
                if let Some(strict) = self.strict {
                    (strict.raw)(keys.previous).map_err(|kind| self.past_head(kind, at))?;
                }
                self.tokens.skip_item()?;
                self.skipped_key = true;
            }
        }
        Ok(true)
    }

    /// Before a variant's name in a map: only definite text names one (D-08).
    #[inline(never)]
    pub(in crate::serde::de) fn variant_name(&mut self) -> Result<(), Error> {
        match self.tokens.initial() {
            Some(initial) if definite_text(initial) => Ok(()),
            Some(_) | None => {
                let (_, at) = self.token()?;
                Err(self.refused(at))
            }
        }
    }

    /// A variant whose form does not fit its kind is a type mismatch: a unit variant is a bare
    /// name, and any other a map (D-08).
    #[inline(never)]
    pub(in crate::serde::de) fn variant_form(&self, fits: bool) -> Result<(), Error> {
        if fits {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::TypeMismatch, self.tokens.offset()))
        }
    }

    /// Whether an externally tagged enum's item is a bare name, rather than a map of one entry.
    #[inline(never)]
    pub(in crate::serde::de) fn unit_variant_next(&self) -> bool {
        self.tokens.initial().is_some_and(definite_text)
    }

    /// The head of an array, entered one level deeper (R-303).
    #[inline(never)]
    pub(in crate::serde::de) fn open_array(&mut self) -> Result<Remaining, Error> {
        match self.token()? {
            (Token::Array(len), at) => self.enter(at, len),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// The head of a map, entered one level deeper (R-303).
    #[inline(never)]
    pub(in crate::serde::de) fn open_map(&mut self) -> Result<Remaining, Error> {
        match self.token()? {
            (Token::Map(len), at) => self.enter(at, len),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// The head of an externally tagged variant's map, which holds exactly one entry, entered.
    #[inline(never)]
    pub(in crate::serde::de) fn open_variant(&mut self) -> Result<Remaining, Error> {
        let at = self.tokens.offset();
        match self.open_map()? {
            Remaining::Count(1) => Ok(Remaining::Count(0)),
            Remaining::UntilBreak => Ok(Remaining::UntilBreak),
            Remaining::Count(_) => Err(self.past_head(ErrorKind::TypeMismatch, at)),
        }
    }

    fn enter(&mut self, at: usize, len: Option<u64>) -> Result<Remaining, Error> {
        self.depth = self
            .depth
            .checked_sub(1)
            .ok_or(Error::new(ErrorKind::LimitExceeded, at))?;
        Ok(len.map_or(Remaining::UntilBreak, Remaining::Count))
    }

    /// Whether another item or entry follows in a container entered with `remaining`.
    #[inline(never)]
    pub(in crate::serde::de) fn more(&mut self, remaining: &mut Remaining) -> Result<bool, Error> {
        use super::BREAK;

        match *remaining {
            Remaining::Count(0) => Ok(false),
            Remaining::Count(left) => {
                *remaining = Remaining::Count(left - 1);
                Ok(true)
            }
            Remaining::UntilBreak if self.tokens.initial() == Some(BREAK) => {
                self.token()?;
                *remaining = Remaining::Count(0);
                Ok(false)
            }
            Remaining::UntilBreak => Ok(true),
        }
    }
    /// Leaves an array or map whose visitor is done, which must have read every item: one left over
    /// is a type mismatch at its head, as natively.
    #[inline(never)]
    pub(in crate::serde::de) fn leave(&mut self, remaining: Remaining) -> Result<(), Error> {
        self.left(remaining, |_, at| Error::new(ErrorKind::TypeMismatch, at))
    }

    /// Leaves an externally tagged variant's map, which must hold no second entry: one is a type
    /// mismatch just past its key's head, as natively.
    #[inline(never)]
    pub(in crate::serde::de) fn leave_variant(
        &mut self,
        remaining: Remaining,
    ) -> Result<(), Error> {
        self.left(remaining, Self::refused)
    }

    fn left(
        &mut self,
        mut remaining: Remaining,
        leftover: fn(&Self, usize) -> Error,
    ) -> Result<(), Error> {
        self.ordered()?;
        let at = self.tokens.offset();
        if self.more(&mut remaining)? {
            Err(leftover(self, at))
        } else {
            self.depth += 1;
            Ok(())
        }
    }
}

/// Whether `initial` begins a text string of definite length.
const fn definite_text(initial: u8) -> bool {
    matches!(Major::of(initial), Major::Text) && initial & 0x1f != crate::wire::INDEFINITE
}
