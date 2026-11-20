#![forbid(unsafe_code)]
//! The next item as `deserialize_any` hands it on, by its own type (#6 Q7).

use crate::serde::{Deserializer, Error};

/// The next item, by the visitor method its type names. An array or map is left unread, for the
/// caller to enter.
pub(in crate::serde::de) enum Any<'de> {
    Unsigned(u64),
    Signed(i64),
    Float(u64),
    Bool(bool),
    Null,
    Text(Value<'de, str>),
    Bytes(Value<'de, [u8]>),
    Array,
    Map,
}

/// A string as it is handed on: borrowed, or with `alloc`, assembled from chunks (D-13a).
#[cfg(feature = "alloc")]
pub(in crate::serde::de) type Value<'de, T> = alloc::borrow::Cow<'de, T>;
#[cfg(not(feature = "alloc"))]
pub(in crate::serde::de) type Value<'de, T> = &'de T;

impl<'de> Deserializer<'de> {
    /// The next item, whatever its type; null is the unit both layers write for `()`, a unit
    /// struct and `None` alike (#6 Q21). A tag but a bignum's, a simple value but a bool or null,
    /// and an integer beyond 64 bits are type mismatches: serde's default `visit_u128` and
    /// `visit_i128`, which its buffering of untagged, internally tagged and `flatten` forms keeps,
    /// format their error with `core::fmt` (#6 Q19).
    #[inline(never)]
    pub(in crate::serde::de) fn next_any(&mut self) -> Result<Any<'de>, Error> {
        use crate::ErrorKind;
        use crate::token::Token;
        use crate::wire::Major;

        match self.tokens.initial().map(Major::of) {
            Some(Major::Array) if !self.skipped_key => return Ok(Any::Array),
            Some(Major::Map) if !self.skipped_key => return Ok(Any::Map),
            Some(Major::Text) => return self.text().map(Any::Text),
            Some(Major::Bytes) => return self.bytes().map(Any::Bytes),
            _ => {}
        }
        match self.token()? {
            (Token::Unsigned(n), _) => Ok(Any::Unsigned(n)),
            (Token::Negative(n), at) => i64::try_from(n)
                .map(|n| Any::Signed(!n))
                .map_err(|_| self.past_head(ErrorKind::TypeMismatch, at)),
            (Token::Tag(2), _) => u64::try_from(self.digits()?)
                .map(Any::Unsigned)
                .map_err(|_| self.here(ErrorKind::TypeMismatch)),
            (Token::Tag(3), _) => i64::try_from(self.digits()?)
                .map(|magnitude| Any::Signed(!magnitude))
                .map_err(|_| self.here(ErrorKind::TypeMismatch)),
            (Token::Float(value), _) => Ok(Any::Float(value.to_bits())),
            (Token::Bool(value), _) => Ok(Any::Bool(value)),
            (Token::Null, _) => Ok(Any::Null),
            (_, at) => Err(self.refused(at)),
        }
    }
}
