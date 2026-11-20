#![forbid(unsafe_code)]

use serde_core::ser::{
    Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};

use super::Serializer;
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
}

impl Compound<'_, '_> {
    fn item<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut *self.serializer)
    }

    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), Error> {
        self.serializer.text(key)?;
        value.serialize(&mut *self.serializer)
    }
}

impl SerializeSeq for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl SerializeTuple for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl SerializeTupleStruct for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl SerializeTupleVariant for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl SerializeMap for Compound<'_, '_> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        self.item(key)
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }

    fn end(self) -> Result<(), Error> {
        Ok(())
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
        Ok(())
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
        Ok(())
    }
}
