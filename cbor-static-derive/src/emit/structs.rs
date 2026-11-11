//! Structs as maps keyed by text, integer or `keys = Label` (D-09), or as positional arrays.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Type;

use super::{Trait, child, impls, key, layout, level, not_yet, static_type};
use crate::Directions;
use crate::model::{Field, Item, Key, StructForm, Unknown};

pub fn structure(
    item: &Item,
    fields: &[Field],
    form: StructForm,
    directions: Directions,
) -> TokenStream {
    let static_ty = static_type(item);
    let layout = layout(
        &format_ident!("LAYOUT"),
        fields.iter().map(|field| {
            let member = &field.member;
            (
                key(&field.key),
                quote!(::core::mem::offset_of!(#static_ty, #member)),
                field.omit_if_absent,
            )
        }),
    );
    let decode_ops = match form {
        StructForm::Map { unknown } => decode_map_ops(fields, unknown),
        StructForm::Array => return not_yet("`#[cbor(array)]`"),
    };
    let types: Vec<&Type> = fields.iter().map(|field| &field.ty).collect();
    let encode = impls(item, directions, Trait::Encode, |path| {
        let (stack, depth) = level(types.iter().map(|ty| child(ty, path)));
        let kinds = types.iter().map(|ty| quote!(<#ty as #path>::ENCODE));
        (
            quote!(__p::EncKind::Struct { ops: &__p::ENCODE_MAP, layout: LAYOUT, kinds: &[#(#kinds),*], field_ptr: None }),
            stack,
            depth,
        )
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        let (stack, depth) = level(types.iter().map(|ty| child(ty, path)));
        let kinds = types.iter().map(|ty| quote!(<#ty as #path>::DECODE));
        (
            quote!(__p::DecKind::Struct { ops: &#decode_ops, layout: LAYOUT, kinds: &[#(#kinds),*], drop: __p::drop_fn::<Self>() }),
            stack,
            depth,
        )
    });
    let labels = labels(fields);
    quote! {
        #layout
        #labels
        #encode
        #decode
    }
}

/// The decode ops for a map of `fields`: matching only the key kinds they use, and skipping or
/// rejecting unknown entries.
pub fn decode_map_ops(fields: &[Field], unknown: Unknown) -> TokenStream {
    let int = fields
        .iter()
        .any(|field| !matches!(field.key, Key::Text(_)));
    let text = fields.iter().any(|field| matches!(field.key, Key::Text(_)));
    match (int, text, unknown) {
        (false, _, Unknown::Skip) => quote!(__p::DECODE_MAP),
        (false, _, Unknown::Deny) => quote!(__p::DECODE_MAP_DENY),
        (true, false, Unknown::Skip) => quote!(__p::DECODE_MAP_INT),
        (true, false, Unknown::Deny) => quote!(__p::DECODE_MAP_INT_DENY),
        (true, true, Unknown::Skip) => quote!(__p::DECODE_MAP_MIXED),
        (true, true, Unknown::Deny) => quote!(__p::DECODE_MAP_MIXED_DENY),
    }
}

/// For `keys = Label`, a match over `Label` that names each field's variant once: a variant
/// without a field is E0004, and a field without a variant E0599 (D-09).
fn labels(fields: &[Field]) -> Option<TokenStream> {
    let (labels, variants): (Vec<_>, Vec<_>) = fields
        .iter()
        .filter_map(|field| match &field.key {
            Key::Label { label, variant } => Some((label, variant)),
            Key::Text(_) | Key::Int(_) => None,
        })
        .unzip();
    labels.first().map(|label| {
        quote!(const _: fn(#label) = |__label| match __label { #(#label::#variants)|* => {} };)
    })
}
