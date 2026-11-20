#![forbid(unsafe_code)]
//! `deserialize_any`: the visitor method the next item's own type names (#6 Q7). serde's
//! untagged, internally tagged and `flatten` forms buffer their items through it (D-10c).

use serde_core::de::Visitor;

use super::Deserializer;
use super::read::{Any, Entries, Value};
use crate::serde::Error;

impl<'de> Deserializer<'de> {
    pub(super) fn any<V: Visitor<'de>>(&mut self, visitor: V) -> Result<V::Value, Error> {
        match self.next_any()? {
            Any::Unsigned(n) => visitor.visit_u64(n),
            Any::Signed(n) => visitor.visit_i64(n),
            Any::Float(bits) => visitor.visit_f64(f64::from_bits(bits)),
            Any::Bool(value) => visitor.visit_bool(value),
            Any::Null => visitor.visit_unit(),
            Any::Text(text) => visit_text(visitor, text),
            Any::Bytes(bytes) => visit_bytes(visitor, bytes),
            Any::Array => self.seq(visitor),
            Any::Map => self.map(Entries::Map, visitor),
        }
    }
}

/// Hands `text` to `visitor`: borrowed from the input, or as owned when it was assembled.
pub(super) fn visit_text<'de, V: Visitor<'de>>(
    visitor: V,
    text: Value<'de, str>,
) -> Result<V::Value, Error> {
    #[cfg(feature = "alloc")]
    return match text {
        alloc::borrow::Cow::Borrowed(text) => visitor.visit_borrowed_str(text),
        alloc::borrow::Cow::Owned(text) => visitor.visit_string(text),
    };
    #[cfg(not(feature = "alloc"))]
    visitor.visit_borrowed_str(text)
}

/// Hands `bytes` to `visitor`, as [`visit_text`] hands text.
pub(super) fn visit_bytes<'de, V: Visitor<'de>>(
    visitor: V,
    bytes: Value<'de, [u8]>,
) -> Result<V::Value, Error> {
    #[cfg(feature = "alloc")]
    return match bytes {
        alloc::borrow::Cow::Borrowed(bytes) => visitor.visit_borrowed_bytes(bytes),
        alloc::borrow::Cow::Owned(bytes) => visitor.visit_byte_buf(bytes),
    };
    #[cfg(not(feature = "alloc"))]
    visitor.visit_borrowed_bytes(bytes)
}
