#![forbid(unsafe_code)]

mod access;
mod any;
mod read;
mod strict;

use serde_core::de::{self, Deserialize, Visitor};

use access::{Map, Seq, Variant};
use any::{visit_bytes, visit_text};
use read::{Entries, Keys};

use crate::serde::Error;
use crate::token::Tokens;

/// The name of no field: serde's field names are text, and these bytes are not UTF-8.
const NOT_UTF8: &[u8] = b"\xff";

/// How deep arrays and maps may nest, unless a [`Deserializer`] is given its own limit (R-303).
const DEFAULT_DEPTH: u8 = 32;

/// A serde Deserializer over [`Tokens`], borrowing definite strings from its input. It applies the
/// native decoder's type checks (D-12) and nests at most 32 arrays and maps deep, or a limit of
/// its own, past which a container is [`crate::ErrorKind::LimitExceeded`] (R-303).
///
/// ```
/// use cbor_static::serde::Deserializer;
/// use cbor_static::ErrorKind;
///
/// let deep = [[0x81; 3].as_slice(), &[0x01]].concat();
/// assert_eq!(Deserializer::new(&deep).decode::<[[[u8; 1]; 1]; 1]>()?, [[[1]]]);
/// assert_eq!(
///     Deserializer::with_depth_limit(&deep, 2).decode::<[[[u8; 1]; 1]; 1]>().map_err(|e| (e.kind, e.offset)),
///     Err((ErrorKind::LimitExceeded, 2))
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// The tokens' own limits on indefinite nesting apply too (RFC 8949 §5.1): 64 open levels, and 8
/// opened while their parent still owes items.
///
/// ```
/// use cbor_static::serde::Deserializer;
///
/// let ignored = |input: &[u8]| {
///     Deserializer::with_depth_limit(input, 255)
///         .decode::<serde::de::IgnoredAny>()
///         .map(|_| ())
///         .map_err(|e| (e.kind, e.offset))
/// };
/// let limited = Err((cbor_static::ErrorKind::LimitExceeded, 65));
/// assert_eq!(ignored(&[[0x9f; 64], [0xff; 64]].concat()), Ok(()));
/// assert_eq!(ignored(&[[0x9f; 65], [0xff; 65]].concat()), limited);
/// let owing = |levels: usize| [b"\x82\x9f".repeat(levels), b"\xff\x00".repeat(levels)].concat();
/// assert_eq!(ignored(&owing(8)), Ok(()));
/// assert_eq!(ignored(&owing(9)).map_err(|(kind, _)| kind), Err(cbor_static::ErrorKind::LimitExceeded));
/// ```
///
/// Driven by hand, it leaves the offset of an error serde raised unplaced, at `u32::MAX`, which
/// [`Deserializer::decode`] and [`Deserializer::decode_partial`] place:
///
/// ```
/// use serde::Deserialize;
/// use cbor_static::serde::Deserializer;
///
/// #[derive(Deserialize, Debug)]
/// struct Point {
///     x: u32,
/// }
///
/// let missing = b"\xa0";
/// let by_hand = Point::deserialize(&mut Deserializer::new(missing)).unwrap_err();
/// assert_eq!(by_hand.offset, u32::MAX);
/// assert_eq!(Deserializer::new(missing).decode::<Point>().unwrap_err().offset, 1);
/// ```
#[derive(Debug)]
pub struct Deserializer<'de> {
    tokens: Tokens<'de>,
    depth: u8,
    skipped_key: bool,
    strict: Option<&'static strict::StrictOps>,
    /// Under Strict, the offset past a map's key that sorts before the key ahead of it, refused when
    /// its value is read, unless it is an unknown field or a duplicate (see [`Self::next_key`]).
    unordered: Option<usize>,
}

impl<'de> Deserializer<'de> {
    /// ```
    /// let deserializer = cbor_static::serde::Deserializer::new(b"\x01");
    /// assert_eq!(deserializer.decode::<u8>()?, 1);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn new(input: &'de [u8]) -> Self {
        Self::with_depth_limit(input, DEFAULT_DEPTH)
    }

    /// A deserializer of deterministic input only (RFC 8949 §4.2.1), as
    /// [`crate::Decoder::new_strict`]: non-preferred heads and floats, bignums that a plain integer
    /// or fewer digits would hold, indefinite lengths and map keys out of bytewise order are
    /// [`crate::ErrorKind::NotDeterministic`], and an entry no field claims is
    /// [`crate::ErrorKind::UnknownField`], as is any item read as serde's `IgnoredAny`. serde's
    /// buffered forms (untagged, internally tagged, `flatten`) decode their entries with serde's
    /// own code, so an unclaimed entry inside them passes (D-10c).
    ///
    /// ```
    /// use cbor_static::serde::Deserializer;
    /// use cbor_static::ErrorKind;
    ///
    /// #[derive(serde::Deserialize, Debug, PartialEq)]
    /// struct Point {
    ///     x: u8,
    ///     y: u8,
    /// }
    ///
    /// let strict = |input| Deserializer::new_strict(input).decode::<Point>().map_err(|e| (e.kind, e.offset));
    /// assert_eq!(strict(b"\xa2\x61x\x01\x61y\x02"), Ok(Point { x: 1, y: 2 }));
    /// assert_eq!(strict(b"\xa2\x61x\x18\x01\x61y\x02"), Err((ErrorKind::NotDeterministic, 5)));
    /// assert_eq!(strict(b"\xa2\x61y\x02\x61x\x01"), Err((ErrorKind::NotDeterministic, 6)));
    /// assert_eq!(
    ///     strict(b"\xa3\x61x\x01\x61y\x02\x61z\x03"),
    ///     Err((ErrorKind::UnknownField, 9))
    /// );
    /// ```
    #[must_use]
    pub const fn new_strict(input: &'de [u8]) -> Self {
        Self {
            strict: Some(&strict::STRICT),
            ..Self::new(input)
        }
    }

    /// A deserializer whose arrays and maps nest at most `limit` deep.
    ///
    /// ```
    /// use cbor_static::serde::Deserializer;
    ///
    /// assert_eq!(Deserializer::with_depth_limit(b"\x80", 1).decode::<[u8; 0]>()?, []);
    /// assert_eq!(
    ///     Deserializer::with_depth_limit(b"\x80", 0).decode::<[u8; 0]>().map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::LimitExceeded)
    /// );
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    #[inline(never)]
    pub const fn with_depth_limit(input: &'de [u8], limit: u8) -> Self {
        Self {
            tokens: Tokens::new(input),
            depth: limit,
            skipped_key: false,
            strict: None,
            unordered: None,
        }
    }

    /// Decodes one item spanning all of the input.
    ///
    /// ```
    /// use cbor_static::serde::Deserializer;
    ///
    /// assert_eq!(Deserializer::new(b"\x63abc").decode::<&str>()?, "abc");
    /// assert_eq!(
    ///     Deserializer::new(b"\x01\x02").decode::<u8>().map_err(|e| (e.kind, e.offset)),
    ///     Err((cbor_static::ErrorKind::TrailingInput, 1))
    /// );
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    /// [`crate::ErrorKind::TrailingInput`] for bytes after the item, and any error decoding it.
    pub fn decode<T: Deserialize<'de>>(self) -> Result<T, crate::Error> {
        self.decoded(true).map(|(value, _)| value)
    }

    /// Decodes the first item of the input, returning the bytes after it: the next item of a CBOR
    /// sequence (RFC 8742).
    ///
    /// ```
    /// use cbor_static::serde::Deserializer;
    ///
    /// assert_eq!(Deserializer::new(b"\x01\x02").decode_partial::<u8>()?, (1, b"\x02".as_slice()));
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    /// Any error decoding the item, at its offset in the input.
    pub fn decode_partial<T: Deserialize<'de>>(self) -> Result<(T, &'de [u8]), crate::Error> {
        self.decoded(false)
    }

    fn decoded<T: Deserialize<'de>>(mut self, whole: bool) -> Result<(T, &'de [u8]), crate::Error> {
        match T::deserialize(&mut self) {
            Ok(value) => self.rest(whole).map(|rest| (value, rest)),
            Err(error) => Err(error.reported(self.tokens.offset())),
        }
    }

    fn seq<V: Visitor<'de>>(&mut self, visitor: V) -> Result<V::Value, Error> {
        let mut seq = Seq {
            remaining: self.open_array()?,
            deserializer: self,
        };
        let value = visitor.visit_seq(&mut seq)?;
        seq.deserializer.leave(seq.remaining)?;
        Ok(value)
    }

    fn map<V: Visitor<'de>>(&mut self, entries: Entries, visitor: V) -> Result<V::Value, Error> {
        let mut map = Map {
            keys: Keys {
                remaining: self.open_map()?,
                entries,
                previous: &[],
            },
            deserializer: self,
        };
        let value = visitor.visit_map(&mut map)?;
        map.deserializer.leave(map.keys.remaining)?;
        Ok(value)
    }
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.any(visitor)
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_bool(self.boolean()?)
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i8(self.integer()?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i16(self.integer()?)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i32(self.integer()?)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i64(self.integer()?)
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i128(self.wide_signed()?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u8(self.integer()?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u16(self.integer()?)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u32(self.integer()?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u64(self.integer()?)
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u128(self.wide_unsigned()?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_f32(f32::from_bits(self.single()?))
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_f64(f64::from_bits(self.double()?))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_char(self.character()?)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visit_text(visitor, self.text()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visit_text(visitor, self.text()?)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visit_bytes(visitor, self.bytes()?)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visit_bytes(visitor, self.bytes()?)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.took_null()? {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.null()?;
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.seq(visitor)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        self.seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.map(Entries::Map, visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.map(Entries::Struct, visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        if self.unit_variant_next() {
            return visitor.visit_enum(Variant {
                deserializer: self,
                payload: false,
            });
        }
        let remaining = self.open_variant()?;
        let value = visitor.visit_enum(Variant {
            deserializer: &mut *self,
            payload: true,
        })?;
        self.leave_variant(remaining)?;
        Ok(value)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.identifier()? {
            Some(name) => visitor.visit_borrowed_str(name),
            None => visitor.visit_borrowed_bytes(NOT_UTF8),
        }
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.ignored()?;
        visitor.visit_unit()
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}
