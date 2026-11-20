#![forbid(unsafe_code)]

use serde_core::ser::{
    Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};

use super::Serializer;
use super::backpatch::Open;
use crate::serde::Error;

/// The items of an array or map whose head [`Serializer`] wrote, which serde hands over one by
/// one: every compound serde writes.
///
/// ```
/// use serde::ser::{SerializeStruct, Serializer as _};
///
/// let mut out = [0u8; 8];
/// let mut serializer = cbor_static::serde::Serializer::new(&mut out);
/// let mut compound = (&mut serializer).serialize_struct("Point", 1)?;
/// compound.serialize_field("x", &1u8)?;
/// compound.end()?;
/// let len = serializer.written();
/// assert_eq!(&out[..len], b"\xa1\x61x\x01");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Debug)]
pub struct Compound<'s, 'a> {
    pub(super) serializer: &'s mut Serializer<'a>,
    pub(super) open: Open,
}

impl Compound<'_, '_> {
    /// An array's item or a map's key, which [`Serializer::counted`] counts.
    fn item<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.serializer.counted(&mut self.open)?;
        value.serialize(&mut *self.serializer)
    }

    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), Error> {
        self.serializer.named(&mut self.open, key)?;
        value.serialize(&mut *self.serializer)
    }

    fn close(self) -> Result<(), Error> {
        self.serializer.close(self.open)
    }

    fn close_declared(self) -> Result<(), Error> {
        self.serializer.close_declared(self.open)
    }
}

impl SerializeSeq for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        self.close()
    }
}

impl SerializeTuple for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        self.close_declared()
    }
}

impl SerializeTupleStruct for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        self.close_declared()
    }
}

impl SerializeTupleVariant for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        self.close_declared()
    }
}

impl SerializeMap for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        self.item(key)
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<(), Error> {
        self.close()
    }
}

impl SerializeStruct for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.field(key, value)
    }

    fn end(self) -> Result<(), Error> {
        self.close_declared()
    }
}

impl SerializeStructVariant for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.field(key, value)
    }

    fn end(self) -> Result<(), Error> {
        self.close_declared()
    }
}
