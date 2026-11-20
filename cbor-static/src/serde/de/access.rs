#![forbid(unsafe_code)]

use serde_core::de::{
    self, DeserializeSeed, Deserializer as _, EnumAccess, MapAccess, SeqAccess, VariantAccess,
};

use super::Deserializer;
use super::read::{Entries, Remaining};
use crate::serde::Error;

/// The items of an array.
pub(super) struct Seq<'a, 'de> {
    pub(super) deserializer: &'a mut Deserializer<'de>,
    pub(super) remaining: Remaining,
}

/// The entries of a map, or of a struct.
pub(super) struct Map<'a, 'de> {
    pub(super) deserializer: &'a mut Deserializer<'de>,
    pub(super) remaining: Remaining,
    pub(super) entries: Entries,
}

/// An externally tagged variant: its bare name, or a map of its name to its payload (D-08).
pub(super) struct Variant<'a, 'de> {
    pub(super) deserializer: &'a mut Deserializer<'de>,
    pub(super) payload: bool,
}

impl<'de> SeqAccess<'de> for Seq<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        if self.deserializer.more(&mut self.remaining)? {
            seed.deserialize(&mut *self.deserializer).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl<'de> MapAccess<'de> for Map<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        if !self
            .deserializer
            .next_key(&mut self.remaining, self.entries)?
        {
            return Ok(None);
        }
        seed.deserialize(&mut *self.deserializer).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        self.deserializer.skipped_key = false;
        seed.deserialize(&mut *self.deserializer)
    }
}

impl<'de> EnumAccess<'de> for Variant<'_, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self), Error> {
        if self.payload {
            self.deserializer.variant_name()?;
        }
        Ok((seed.deserialize(&mut *self.deserializer)?, self))
    }
}

impl<'de> VariantAccess<'de> for Variant<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        self.deserializer.variant_form(!self.payload)
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        self.deserializer.variant_form(self.payload)?;
        seed.deserialize(self.deserializer)
    }

    fn tuple_variant<V: de::Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        self.deserializer.variant_form(self.payload)?;
        self.deserializer.deserialize_tuple(len, visitor)
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserializer.variant_form(self.payload)?;
        self.deserializer.deserialize_struct("", fields, visitor)
    }
}
