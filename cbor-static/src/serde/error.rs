#![forbid(unsafe_code)]

use core::fmt;

use serde_core::{de, ser};

use crate::error::ErrorKind;

/// The serde layer's error: an [`ErrorKind`] and the byte offset reached, with no field index,
/// which a serde format never learns (D-23a). The entry points return it as a [`crate::Error`].
///
/// ```
/// use cbor_static::ErrorKind;
///
/// let error = cbor_static::serde::Error::from(cbor_static::Error::from(ErrorKind::TypeMismatch));
/// assert_eq!((error.kind, error.offset), (ErrorKind::TypeMismatch, 0));
/// assert_eq!(size_of::<cbor_static::serde::Error>(), 8);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: u32,
}

/// The offset of an error raised where no position is known, until an entry point places it.
const UNPLACED: u32 = u32::MAX;

impl Error {
    pub(crate) fn new(kind: ErrorKind, offset: usize) -> Self {
        Self {
            kind,
            offset: u32::try_from(offset).unwrap_or(UNPLACED),
        }
    }

    /// What an entry point returns for this error, where encoding or decoding stopped at `at`.
    #[inline(never)]
    pub(crate) fn reported(self, at: usize) -> crate::Error {
        self.placed(at).into()
    }

    /// This error at `at`, unless it already has an offset of its own.
    pub(crate) fn placed(self, at: usize) -> Self {
        match self.offset {
            UNPLACED => Self::new(self.kind, at),
            _ => self,
        }
    }
}

const fn unplaced(kind: ErrorKind) -> Error {
    Error {
        kind,
        offset: UNPLACED,
    }
}

/// ```
/// use cbor_static::ErrorKind;
///
/// let error = cbor_static::Error::from(ErrorKind::OutOfRange);
/// assert_eq!(cbor_static::Error::from(cbor_static::serde::Error::from(error)), error);
/// ```
impl From<crate::Error> for Error {
    fn from(error: crate::Error) -> Self {
        Self {
            kind: error.kind,
            offset: error.offset,
        }
    }
}

/// With field index 0.
///
/// ```
/// use serde::de::Error as _;
///
/// let error = cbor_static::Error::from(cbor_static::serde::Error::missing_field("x"));
/// assert_eq!((error.kind, error.field), (cbor_static::ErrorKind::MissingField, 0));
/// ```
impl From<Error> for crate::Error {
    fn from(error: Error) -> Self {
        Self {
            kind: error.kind,
            field: 0,
            offset: error.offset,
        }
    }
}

/// The kind's name, without its offset or field, so no number is formatted (D-23).
///
/// ```
/// use cbor_static::{Error, ErrorKind};
///
/// assert_eq!(Error::from(ErrorKind::TypeMismatch).to_string(), "type mismatch");
/// let source: &dyn core::error::Error = &Error::from(ErrorKind::Custom);
/// assert_eq!(source.to_string(), "custom");
/// ```
impl fmt::Display for crate::Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(name(self.kind))
    }
}

impl core::error::Error for crate::Error {}

/// As [`crate::Error`]'s.
///
/// ```
/// use serde::de::Error as _;
///
/// let source: &dyn core::error::Error = &cbor_static::serde::Error::unknown_field("z", &[]);
/// assert_eq!(source.to_string(), "unknown field");
/// ```
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(name(self.kind))
    }
}

impl core::error::Error for Error {}

/// `custom` drops its message unformatted.
///
/// ```
/// use serde::ser::Error as _;
///
/// assert_eq!(cbor_static::serde::Error::custom("refused").kind, cbor_static::ErrorKind::Custom);
/// ```
impl ser::Error for Error {
    fn custom<T: fmt::Display>(_: T) -> Self {
        unplaced(ErrorKind::Custom)
    }
}

/// Each of serde's errors maps to a kind, its message dropped unformatted.
///
/// ```
/// use serde::de::{Error as _, Unexpected};
/// use cbor_static::ErrorKind;
/// use cbor_static::serde::Error;
///
/// let kinds = [
///     Error::custom("refused"),
///     Error::invalid_type(Unexpected::Bool(true), &"a string"),
///     Error::invalid_value(Unexpected::Str("ab"), &"a character"),
///     Error::invalid_length(1, &"two elements"),
///     Error::unknown_variant("D", &["A", "B"]),
///     Error::unknown_field("z", &["x"]),
///     Error::missing_field("x"),
///     Error::duplicate_field("x"),
/// ]
/// .map(|error| error.kind);
/// assert_eq!(
///     kinds,
///     [
///         ErrorKind::Custom,
///         ErrorKind::TypeMismatch,
///         ErrorKind::TypeMismatch,
///         ErrorKind::MissingField,
///         ErrorKind::UnknownVariant,
///         ErrorKind::UnknownField,
///         ErrorKind::MissingField,
///         ErrorKind::DuplicateKey,
///     ]
/// );
/// ```
impl de::Error for Error {
    fn custom<T: fmt::Display>(_: T) -> Self {
        unplaced(ErrorKind::Custom)
    }

    fn invalid_type(_: de::Unexpected, _: &dyn de::Expected) -> Self {
        unplaced(ErrorKind::TypeMismatch)
    }

    fn invalid_value(_: de::Unexpected, _: &dyn de::Expected) -> Self {
        unplaced(ErrorKind::TypeMismatch)
    }

    fn invalid_length(_: usize, _: &dyn de::Expected) -> Self {
        unplaced(ErrorKind::MissingField)
    }

    fn unknown_variant(_: &str, _: &'static [&'static str]) -> Self {
        unplaced(ErrorKind::UnknownVariant)
    }

    fn unknown_field(_: &str, _: &'static [&'static str]) -> Self {
        unplaced(ErrorKind::UnknownField)
    }

    fn missing_field(_: &'static str) -> Self {
        unplaced(ErrorKind::MissingField)
    }

    fn duplicate_field(_: &'static str) -> Self {
        unplaced(ErrorKind::DuplicateKey)
    }
}

const fn name(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::UnexpectedEnd => "unexpected end",
        ErrorKind::NotWellFormed => "not well-formed",
        ErrorKind::TypeMismatch => "type mismatch",
        ErrorKind::OutOfRange => "out of range",
        ErrorKind::InvalidUtf8 => "invalid UTF-8",
        ErrorKind::DuplicateKey => "duplicate key",
        ErrorKind::MissingField => "missing field",
        ErrorKind::UnknownVariant => "unknown variant",
        ErrorKind::UnexpectedTag => "unexpected tag",
        ErrorKind::IndefiniteString => "indefinite string",
        ErrorKind::NeedContiguous => "need contiguous input",
        ErrorKind::Unsupported => "unsupported",
        ErrorKind::LimitExceeded => "limit exceeded",
        ErrorKind::BufferTooSmall => "buffer too small",
        ErrorKind::TrailingInput => "trailing input",
        ErrorKind::UnknownField => "unknown field",
        ErrorKind::NotDeterministic => "not deterministic",
        ErrorKind::OutOfMemory => "out of memory",
        ErrorKind::Custom => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, UNPLACED, unplaced};
    use crate::error::ErrorKind;

    #[test]
    fn placing_fills_only_an_unplaced_offset() {
        assert_eq!(unplaced(ErrorKind::Custom).offset, UNPLACED);
        assert_eq!(unplaced(ErrorKind::Custom).placed(5).offset, 5);
        assert_eq!(unplaced(ErrorKind::Custom).placed(5).placed(9).offset, 5);
        assert_eq!(Error::new(ErrorKind::TypeMismatch, 3).placed(9).offset, 3);
    }
}
