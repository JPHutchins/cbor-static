//! Structs as maps keyed by text, integer or `keys = Label` (D-09), or as positional arrays.

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};

use super::{Body, Trait, field_child, field_kind, impls, key, layout, level, static_type};
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
    let (encode_ops, decode_ops) = match form {
        StructForm::Map { unknown } => (quote!(__p::ENCODE_MAP), decode_map_ops(fields, unknown)),
        StructForm::Array => (quote!(__p::ENCODE_ARRAY), quote!(__p::DECODE_ARRAY)),
    };
    let lead = match form {
        StructForm::Map { .. } => quote!(__p::Lead::MAP),
        StructForm::Array => quote!(__p::Lead::ARRAY),
    };
    let encode = impls(item, directions, Trait::Encode, |path| {
        let (stack, depth) = level(
            fields
                .iter()
                .map(|field| field_child(&field.ty, field.bytes, path)),
        );
        let kinds = fields
            .iter()
            .map(|field| field_kind(&field.ty, field.bytes, Trait::Encode, path));
        Body {
            kind: quote!(&__p::EncKind::Struct { ops: &#encode_ops, layout: LAYOUT, kinds: &[#(#kinds),*], field_ptr: None }),
            stack,
            depth,
            lead: lead.clone(),
        }
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        let (stack, depth) = level(
            fields
                .iter()
                .map(|field| field_child(&field.ty, field.bytes, path)),
        );
        let kinds = fields
            .iter()
            .map(|field| field_kind(&field.ty, field.bytes, Trait::Decode, path));
        Body {
            kind: quote!(&__p::DecKind::Struct { ops: &#decode_ops, layout: LAYOUT, kinds: &[#(#kinds),*], drop: __p::drop_fn::<Self>() }),
            stack,
            depth,
            lead,
        }
    });
    let labels = labels(fields);
    quote! {
        #layout
        #labels
        #encode
        #decode
    }
}

/// A unit struct, as `()`, or a newtype struct, as its `field` (D-08): the wrapped type's
/// descriptor, behind a const assert that the struct is laid out as that type. A decoded value
/// drops as the wrapper, which may have its own `Drop`.
pub fn transparent(item: &Item, field: Option<&Field>, directions: Directions) -> TokenStream {
    let (ty, bytes, offset) = field.map_or_else(
        || (quote!(()), false, quote!(0)),
        |field| {
            (
                field.ty.to_token_stream(),
                field.bytes,
                quote!(::core::mem::offset_of!(Self, 0)),
            )
        },
    );
    let wraps = quote!(assert!(
        __p::wraps::<Self, #ty>(#offset),
        "cbor-static: a unit or newtype struct must be laid out as what it wraps"
    ););
    let encode = impls(item, directions, Trait::Encode, |path| {
        let (stack, depth) = field_child(&ty, bytes, path);
        let kind = field_kind(&ty, bytes, Trait::Encode, path);
        Body {
            kind: quote!({ #wraps #kind }),
            stack,
            depth,
            lead: TokenStream::new(),
        }
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        let (stack, depth) = field_child(&ty, bytes, path);
        let kind = field_kind(&ty, bytes, Trait::Decode, path);
        Body {
            kind: quote!({
                #wraps
                if ::core::mem::needs_drop::<Self>() {
                    &#kind.dropped_by(__p::drop_fn::<Self>())
                } else {
                    #kind
                }
            }),
            stack,
            depth,
            lead: if bytes {
                quote!(__p::Lead::BYTES)
            } else {
                quote!(<#ty as #path>::LEAD)
            },
        }
    });
    quote! {
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
            Key::Text(_) | Key::Int(_) | Key::Position => None,
        })
        .unzip();
    labels.first().map(|label| {
        quote!(const _: fn(#label) = |__label| match __label { #(#label::#variants)|* => {} };)
    })
}
