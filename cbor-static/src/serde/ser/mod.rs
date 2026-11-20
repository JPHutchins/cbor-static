#![forbid(unsafe_code)]

mod backpatch;
mod compound;

use serde_core::ser::{self, Serialize};

pub use compound::Compound;

use crate::ErrorKind;
use crate::serde::Error;
use crate::wire::{Head, Major, float};

/// A serde Serializer of preferred serialization (RFC 8949 §4.1), with definite lengths only
/// (D-11), into a slice. A sequence, map or text of unknown length gets its length when it closes.
///
/// ```
/// use serde::Serialize;
///
/// let mut out = [0u8; 8];
/// let mut serializer = cbor_static::serde::Serializer::new(&mut out);
/// (1.5f64, "a").serialize(&mut serializer)?;
/// let len = serializer.written();
/// assert_eq!(&out[..len], b"\x82\xf9\x3e\x00\x61a");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug)]
pub struct Serializer<'a> {
    sink: Sink<'a>,
    at: usize,
}

/// Where the bytes go: nowhere, while the length of an encoding is measured, or into a slice.
#[derive(Debug)]
enum Sink<'a> {
    #[cfg(feature = "alloc")]
    Measure,
    Write(&'a mut [u8]),
}

impl<'a> Serializer<'a> {
    /// ```
    /// assert_eq!(cbor_static::serde::Serializer::new(&mut [0u8; 4]).written(), 0);
    /// ```
    #[must_use]
    pub const fn new(out: &'a mut [u8]) -> Self {
        Self {
            sink: Sink::Write(out),
            at: 0,
        }
    }

    /// ```
    /// use serde::Serialize;
    ///
    /// let mut out = [0u8; 4];
    /// let mut serializer = cbor_static::serde::Serializer::new(&mut out);
    /// "abc".serialize(&mut serializer)?;
    /// assert_eq!(serializer.written(), 4);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn written(&self) -> usize {
        self.at
    }
}

impl<'s, 'a> ser::Serializer for &'s mut Serializer<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Compound<'s, 'a>;
    type SerializeTuple = Compound<'s, 'a>;
    type SerializeTupleStruct = Compound<'s, 'a>;
    type SerializeTupleVariant = Compound<'s, 'a>;
    type SerializeMap = Compound<'s, 'a>;
    type SerializeStruct = Compound<'s, 'a>;
    type SerializeStructVariant = Compound<'s, 'a>;

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        use crate::wire::{FALSE, TRUE};

        self.head(Major::Simple, if v { TRUE } else { FALSE }.into())
    }

    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.signed(v.into())
    }

    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.signed(v.into())
    }

    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.signed(v.into())
    }

    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.signed(v)
    }

    fn serialize_i128(self, v: i128) -> Result<(), Error> {
        match v {
            0.. => self.bignum(Major::Uint, 2, v.cast_unsigned()),
            _ => self.bignum(Major::Nint, 3, (!v).cast_unsigned()),
        }
    }

    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        self.head(Major::Uint, v.into())
    }

    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        self.head(Major::Uint, v.into())
    }

    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        self.head(Major::Uint, v.into())
    }

    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.head(Major::Uint, v)
    }

    fn serialize_u128(self, v: u128) -> Result<(), Error> {
        self.bignum(Major::Uint, 2, v)
    }

    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        self.put(
            Head::float(float::shortest(float::from_single(v.to_bits()))).as_bytes(),
            &[],
        )
    }

    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        self.put(Head::float(float::shortest(v.to_bits())).as_bytes(), &[])
    }

    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.text(v.encode_utf8(&mut [0; 4]))
    }

    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.text(v)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        self.put(Head::new(Major::Bytes, v.len() as u64).as_bytes(), v)
    }

    fn serialize_none(self) -> Result<(), Error> {
        use crate::wire::NULL;

        self.head(Major::Simple, NULL.into())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), Error> {
        self.serialize_none()
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        self.serialize_none()
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), Error> {
        self.text(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.variant(variant)?;
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Compound<'s, 'a>, Error> {
        self.compound(Major::Array, len)
    }

    fn serialize_tuple(self, len: usize) -> Result<Compound<'s, 'a>, Error> {
        self.compound(Major::Array, Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        len: usize,
    ) -> Result<Compound<'s, 'a>, Error> {
        self.compound(Major::Array, Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Compound<'s, 'a>, Error> {
        self.variant(variant)?;
        self.compound(Major::Array, Some(len))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Compound<'s, 'a>, Error> {
        self.compound(Major::Map, len)
    }

    fn serialize_struct(self, _: &'static str, len: usize) -> Result<Compound<'s, 'a>, Error> {
        self.compound(Major::Map, Some(len))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Compound<'s, 'a>, Error> {
        self.variant(variant)?;
        self.compound(Major::Map, Some(len))
    }

    fn collect_str<T: ?Sized + core::fmt::Display>(self, value: &T) -> Result<(), Error> {
        self.formatted(format_args!("{value}"))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl<'a> Serializer<'a> {
    /// Appends `head` and `payload`, or when measuring, only counts them. When they do not fit,
    /// the error is at the end of the buffer, where the native encoder stops.
    #[inline(never)]
    fn put(&mut self, head: &[u8], payload: &[u8]) -> Result<(), Error> {
        let len = head.len() + payload.len();
        let room = match &mut self.sink {
            #[cfg(feature = "alloc")]
            Sink::Measure => len,
            Sink::Write(out) => {
                let rest = out.get_mut(self.at..).unwrap_or_default();
                rest.iter_mut()
                    .zip(head.iter().chain(payload))
                    .for_each(|(slot, byte)| *slot = *byte);
                rest.len()
            }
        };
        if room < len {
            return Err(Error::new(ErrorKind::BufferTooSmall, self.at + room));
        }
        self.at += len;
        Ok(())
    }

    #[inline(never)]
    fn head(&mut self, major: Major, arg: u64) -> Result<(), Error> {
        self.put(Head::new(major, arg).as_bytes(), &[])
    }

    #[inline(never)]
    fn text(&mut self, text: &str) -> Result<(), Error> {
        self.put(
            Head::new(Major::Text, text.len() as u64).as_bytes(),
            text.as_bytes(),
        )
    }

    /// The opening of every variant but a unit one: a map of one entry, keyed by its name.
    #[inline(never)]
    fn variant(&mut self, name: &str) -> Result<(), Error> {
        self.head(Major::Map, 1)?;
        self.text(name)
    }

    #[inline(never)]
    fn signed(&mut self, value: i64) -> Result<(), Error> {
        match value {
            0.. => self.head(Major::Uint, value.cast_unsigned()),
            _ => self.head(Major::Nint, (!value).cast_unsigned()),
        }
    }

    /// A plain integer while the magnitude fits a `u64`, then bignum `tag` (RFC 8949 §3.4.3).
    #[inline(never)]
    fn bignum(&mut self, major: Major, tag: u64, magnitude: u128) -> Result<(), Error> {
        if let Ok(small) = u64::try_from(magnitude) {
            return self.head(major, small);
        }
        let be = magnitude.to_be_bytes();
        let digits = be
            .get(usize::try_from(magnitude.leading_zeros() / 8).unwrap_or(0)..)
            .unwrap_or(&be);
        self.head(Major::Tag, tag)?;
        self.put(
            Head::new(Major::Bytes, digits.len() as u64).as_bytes(),
            digits,
        )
    }

    fn compound(&mut self, major: Major, len: Option<usize>) -> Result<Compound<'_, 'a>, Error> {
        Ok(Compound {
            open: self.open(major, len)?,
            serializer: self,
        })
    }
}

#[cfg(feature = "alloc")]
impl Serializer<'static> {
    /// A serializer that writes nothing, to learn an encoding's length.
    pub(crate) const fn measuring() -> Self {
        Self {
            sink: Sink::Measure,
            at: 0,
        }
    }
}
