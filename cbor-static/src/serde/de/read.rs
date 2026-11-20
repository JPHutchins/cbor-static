#![forbid(unsafe_code)]
//! The reads under every generic method of the Deserializer, non-generic or generic over an
//! integer's width only (R-301).

use super::Deserializer;
use crate::ErrorKind;
use crate::serde::Error;
use crate::token::Token;
use crate::wire::Major;

/// Whose entries a map holds: any keys, or a struct's fields, whose keys are text.
#[derive(Clone, Copy, Debug)]
pub(super) enum Entries {
    Map,
    Struct,
}

/// What remains of an array or map: a count of items or entries, or items until a break.
#[derive(Clone, Copy, Debug)]
pub(super) enum Remaining {
    Count(u64),
    UntilBreak,
}

/// The initial byte of a break, which ends an indefinite-length item.
const BREAK: u8 = simple(crate::wire::INDEFINITE);
const NULL: u8 = simple(crate::wire::NULL);

/// The initial byte of the head of simple value or break `info`.
const fn simple(info: u8) -> u8 {
    crate::wire::Header {
        major: Major::Simple,
        info,
        arg: 0,
    }
    .initial()
}

impl<'de> Deserializer<'de> {
    /// The next token, and the offset of its head. A struct's key that is not text, which
    /// [`Self::next_key`] skipped, can be read only as an identifier or ignored.
    fn token(&mut self) -> Result<(Token<'de>, usize), Error> {
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
    fn past_head(&self, kind: ErrorKind, at: usize) -> Error {
        Error::new(kind, self.tokens.head_end(at))
    }

    /// A type mismatch just past the head at `at`, as natively, which also refuses a tag there.
    fn refused(&self, at: usize) -> Error {
        self.past_head(ErrorKind::TypeMismatch, at)
    }

    /// `error` from reading a string the native decoder copies rather than borrows (a key, a name,
    /// a character, a bignum's digits): one cut short is reported at the end of the input.
    fn copied(&self, error: Error) -> Error {
        match error.kind {
            ErrorKind::UnexpectedEnd => self.here(error.kind),
            _ => error,
        }
    }

    /// `kind` where reading has reached: past a string, which the native decoder also takes whole
    /// before it refuses it as the other kind of string, or as a character.
    fn here(&self, kind: ErrorKind) -> Error {
        Error::new(kind, self.tokens.offset())
    }

    #[inline(never)]
    pub(super) fn boolean(&mut self) -> Result<bool, Error> {
        match self.token()? {
            (Token::Bool(value), _) => Ok(value),
            (_, at) => Err(self.refused(at)),
        }
    }

    #[inline(never)]
    pub(super) fn null(&mut self) -> Result<(), Error> {
        match self.token()? {
            (Token::Null, _) => Ok(()),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// Moves past the next item, or past nothing when it is a struct's key that is not text,
    /// which [`Self::next_key`] skipped already.
    #[inline(never)]
    pub(super) fn ignored(&mut self) -> Result<(), Error> {
        if core::mem::take(&mut self.skipped_key) {
            return Ok(());
        }
        Ok(self.tokens.skip_item()?)
    }

    /// Whether the next item is null, which it then reads.
    #[inline(never)]
    pub(super) fn took_null(&mut self) -> Result<bool, Error> {
        match self.tokens.initial() {
            Some(NULL) => self.null().map(|()| true),
            _ => Ok(false),
        }
    }

    /// An integer as `N`, which floats are not (D-12).
    #[inline(never)]
    pub(super) fn integer<N: TryFrom<i128>>(&mut self) -> Result<N, Error> {
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
    pub(super) fn wide_unsigned(&mut self) -> Result<u128, Error> {
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
    pub(super) fn wide_signed(&mut self) -> Result<i128, Error> {
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

    /// A bignum's magnitude: a byte string of at most 16 digits past its leading zeros.
    fn digits(&mut self) -> Result<u128, Error> {
        match self.token().map_err(|error| self.copied(error))? {
            (Token::Bytes(digits), _) => {
                let significant = digits
                    .get(digits.iter().take_while(|&&digit| digit == 0).count()..)
                    .unwrap_or_default();
                match significant.len() {
                    0..=16 => Ok(significant
                        .iter()
                        .fold(0, |magnitude, &digit| magnitude << 8 | u128::from(digit))),
                    _ => Err(self.here(ErrorKind::OutOfRange)),
                }
            }
            (Token::BytesIndefinite, at) => Err(self.past_head(ErrorKind::IndefiniteString, at)),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// A float's binary64 bits, which integers are not (D-12).
    #[inline(never)]
    pub(super) fn double(&mut self) -> Result<u64, Error> {
        match self.token()? {
            (Token::Float(value), _) => Ok(value.to_bits()),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// A float's binary32 bits, when it narrows exactly (D-12).
    #[inline(never)]
    pub(super) fn single(&mut self) -> Result<u32, Error> {
        let at = self.tokens.offset();
        crate::wire::float::to_single(self.double()?)
            .ok_or(self.past_head(ErrorKind::OutOfRange, at))
    }

    #[inline(never)]
    pub(super) fn text(&mut self) -> Result<&'de str, Error> {
        match self.token()? {
            (Token::Text(text), _) => Ok(text),
            (Token::Bytes(_), _) => Err(self.here(ErrorKind::TypeMismatch)),
            (Token::TextIndefinite, at) => Err(self.past_head(ErrorKind::IndefiniteString, at)),
            (_, at) => Err(self.refused(at)),
        }
    }

    #[inline(never)]
    pub(super) fn bytes(&mut self) -> Result<&'de [u8], Error> {
        match self.token()? {
            (Token::Bytes(bytes), _) => Ok(bytes),
            (Token::Text(_), _) => Err(self.here(ErrorKind::TypeMismatch)),
            (Token::BytesIndefinite, at) => Err(self.past_head(ErrorKind::IndefiniteString, at)),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// Text of exactly one character.
    #[inline(never)]
    pub(super) fn character(&mut self) -> Result<char, Error> {
        let mut chars = self.text().map_err(|error| self.copied(error))?.chars();
        match (chars.next(), chars.next()) {
            (Some(only), None) => Ok(only),
            _ => Err(self.here(ErrorKind::TypeMismatch)),
        }
    }

    /// A struct's key or a variant's name, or `None` in place of a struct's key that is not text,
    /// which [`Self::next_key`] skipped.
    #[inline(never)]
    pub(super) fn identifier(&mut self) -> Result<Option<&'de str>, Error> {
        if core::mem::take(&mut self.skipped_key) {
            return Ok(None);
        }
        self.text().map(Some).map_err(|error| match error.kind {
            ErrorKind::IndefiniteString => Error {
                kind: ErrorKind::Unsupported,
                ..error
            },
            _ => self.copied(error),
        })
    }

    /// Whether another entry follows. Before a struct's key, one that is not text is skipped, so
    /// [`Self::identifier`] names no field in its place (D-12).
    #[inline(never)]
    pub(super) fn next_key(
        &mut self,
        remaining: &mut Remaining,
        entries: Entries,
    ) -> Result<bool, Error> {
        if !self.more(remaining)? {
            return Ok(false);
        }
        match (entries, self.tokens.initial().map(Major::of)) {
            (Entries::Map, _) | (Entries::Struct, Some(Major::Text) | None) => {}
            (Entries::Struct, Some(_)) => {
                self.tokens.skip_item()?;
                self.skipped_key = true;
            }
        }
        Ok(true)
    }

    /// Before a variant's name in a map: only definite text names one (D-08).
    #[inline(never)]
    pub(super) fn variant_name(&mut self) -> Result<(), Error> {
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
    pub(super) fn variant_form(&self, fits: bool) -> Result<(), Error> {
        if fits {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::TypeMismatch, self.tokens.offset()))
        }
    }

    /// Whether an externally tagged enum's item is a bare name, rather than a map of one entry.
    #[inline(never)]
    pub(super) fn unit_variant_next(&self) -> bool {
        self.tokens.initial().is_some_and(definite_text)
    }

    /// The head of an array, entered one level deeper (R-303).
    #[inline(never)]
    pub(super) fn open_array(&mut self) -> Result<Remaining, Error> {
        match self.token()? {
            (Token::Array(len), at) => self.enter(at, len),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// The head of a map, entered one level deeper (R-303).
    #[inline(never)]
    pub(super) fn open_map(&mut self) -> Result<Remaining, Error> {
        match self.token()? {
            (Token::Map(len), at) => self.enter(at, len),
            (_, at) => Err(self.refused(at)),
        }
    }

    /// The head of an externally tagged variant's map, which holds exactly one entry, entered.
    #[inline(never)]
    pub(super) fn open_variant(&mut self) -> Result<Remaining, Error> {
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
    pub(super) fn more(&mut self, remaining: &mut Remaining) -> Result<bool, Error> {
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

    /// The input after an item decoded, which must be empty when the item was to span it `whole`.
    #[inline(never)]
    pub(super) fn rest(&self, whole: bool) -> Result<&'de [u8], crate::Error> {
        match self.tokens.rest() {
            [_, ..] if whole => {
                Err(Error::new(ErrorKind::TrailingInput, self.tokens.offset()).into())
            }
            rest => Ok(rest),
        }
    }

    /// Leaves an array or map whose visitor is done, which must have read every item: one left over
    /// is a type mismatch at its head, as natively.
    #[inline(never)]
    pub(super) fn leave(&mut self, remaining: Remaining) -> Result<(), Error> {
        self.left(remaining, |_, at| Error::new(ErrorKind::TypeMismatch, at))
    }

    /// Leaves an externally tagged variant's map, which must hold no second entry: one is a type
    /// mismatch just past its key's head, as natively.
    #[inline(never)]
    pub(super) fn leave_variant(&mut self, remaining: Remaining) -> Result<(), Error> {
        self.left(remaining, Self::refused)
    }

    fn left(
        &mut self,
        mut remaining: Remaining,
        leftover: fn(&Self, usize) -> Error,
    ) -> Result<(), Error> {
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
