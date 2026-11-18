//! `Decode::CONTIGUOUS` (#5 Q5): an internally or adjacently tagged enum's own, or else whether
//! any field it decodes is.

use proc_macro2::TokenStream;
use quote::quote;

use crate::model::{EnumForm, Field, Key, Payload, Shape};

pub fn contiguous(shape: &Shape) -> TokenStream {
    match shape {
        Shape::Enum {
            form: EnumForm::Internal { .. } | EnumForm::Adjacent { .. },
            ..
        } => quote!(true),
        Shape::Enum { variants, .. } => {
            any(variants.iter().flat_map(|variant| match &variant.payload {
                Payload::Unit => &[],
                Payload::Newtype(field) => core::slice::from_ref(field.as_ref()),
                Payload::Struct(fields) => fields.as_slice(),
            }))
        }
        Shape::Struct { fields, .. } => any(fields.iter()),
        Shape::Newtype { field, .. } => any(core::iter::once(field.as_ref())),
        Shape::Unit => any(core::iter::empty()),
    }
}

fn any<'a>(fields: impl Iterator<Item = &'a Field>) -> TokenStream {
    let decoded = fields.filter_map(field);
    quote!(false #(|| #decoded)*)
}

/// A field's own: none for a `skip`ped field, which is not decoded, or a byte string, which is
/// copied.
fn field(field: &Field) -> Option<TokenStream> {
    let ty = &field.ty;
    match (&field.key, field.bytes) {
        (Key::Skipped, _) | (_, true) => None,
        (Key::Other, false) => Some(quote!(<#ty as __p::Other>::CONTIGUOUS)),
        (
            Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position | Key::Flattened,
            false,
        ) => Some(quote!(<#ty as ::cbor_static::Decode<'de>>::CONTIGUOUS)),
    }
}
