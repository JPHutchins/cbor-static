//! Structs as maps keyed by text, integer or `keys = Label` (D-09), or as positional arrays.

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};

use super::{
    Body, Order, Trait, admitted, arranged, derives, field_child, field_decode, field_encode,
    field_kind, field_stack, impls, joined, key, layout, level, makers, ordered, part_order,
    static_type,
};
use crate::Directions;
use crate::model::{Field, Item, Key, Mode, StructForm, Unknown};

pub fn structure(
    item: &Item,
    fields: &[Field],
    (form, mode): (StructForm, Mode),
    directions: Directions,
) -> TokenStream {
    let static_ty = static_type(item);
    let name = format_ident!("LAYOUT");
    let sort = match form {
        StructForm::Map { .. } => mode,
        StructForm::Array => Mode::Declared,
    };
    let layout = layout(
        &name,
        fields.iter().map(|field| {
            let member = &field.member;
            (
                key(&field.key),
                quote!(::core::mem::offset_of!(#static_ty, #member)),
                field.omit_if_absent,
            )
        }),
        sort,
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
        let (stack, depth) = level(fields.iter().map(|field| field_stack(field, path)));
        let kinds = arranged(
            &name,
            sort,
            fields.iter().map(|field| field_encode(field, path)),
        );
        let parts = fields
            .iter()
            .filter_map(|field| Some((member_name(field), part_order(field, path)?)));
        let checks = admitted(mode, parts.clone());
        let kind = quote!(__p::EncKind::Struct { ops: &#encode_ops, layout: LAYOUT, kinds: #kinds, field_ptr: None });
        Body {
            kind: if checks.is_empty() {
                quote!(&#kind)
            } else {
                quote!(&{ #checks #kind })
            },
            stack,
            depth,
            lead: lead.clone(),
            order: match form {
                StructForm::Map { .. } => ordered(mode),
                StructForm::Array => joined(mode, parts.map(|(_, part)| part)),
            },
        }
    });
    let generics = item.lifetime.as_ref().map(|lifetime| quote!(<#lifetime>));
    let decode = impls(item, directions, Trait::Decode, |path| {
        let (stack, depth) = level(fields.iter().map(|field| field_stack(field, path)));
        let kinds = arranged(
            &name,
            sort,
            fields.iter().enumerate().map(|(index, field)| {
                field_decode(field, index, &item.ident, generics.as_ref(), path)
            }),
        );
        Body {
            kind: quote!(&__p::DecKind::Struct { ops: &#decode_ops, layout: LAYOUT, kinds: #kinds, drop: __p::drop_fn::<Self>() }),
            stack,
            depth,
            lead,
            order: Order::default(),
        }
    });
    let labels = labels(fields);
    let map_form = [
        (Trait::Encode, quote!(__p::Encoding)),
        (Trait::Decode, quote!(__p::Decoding)),
    ]
    .into_iter()
    .filter(|(which, _)| matches!(form, StructForm::Map { .. }) && derives(directions, *which))
    .map(|(_, direction)| {
        let ident = &item.ident;
        quote!(impl #generics __p::MapForm<#direction> for #ident #generics {})
    });
    let makers = makers(
        &item.ident,
        fields,
        generics.as_ref(),
        item.lifetime
            .as_ref()
            .map(|lifetime| quote!(pub __marker: ::core::marker::PhantomData<&#lifetime ()>,))
            .as_ref(),
    );
    quote! {
        #layout
        #labels
        #(#map_form)*
        #makers
        #encode
        #decode
    }
}

/// A unit struct, as `()`, or a newtype or `transparent` struct, as its `field` (D-08): the wrapped
/// type's descriptor, behind a const assert that the struct is laid out as that type. A decoded
/// value drops as the wrapper, which may have its own `Drop`. It commits to its field's key order,
/// or in `mode`, to that one, which makes it a deterministic root of the field.
pub fn transparent(
    item: &Item,
    field: Option<&Field>,
    mode: Mode,
    directions: Directions,
) -> TokenStream {
    let (ty, bytes, offset) = field.map_or_else(
        || (quote!(()), false, quote!(0)),
        |field| {
            let member = &field.member;
            (
                field.ty.to_token_stream(),
                field.bytes,
                quote!(::core::mem::offset_of!(Self, #member)),
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
        let part = (!bytes).then(|| quote!(<#ty as #path>::ORDER));
        let checks = admitted(
            mode,
            field
                .zip(part.clone())
                .map(|(field, part)| (member_name(field), part))
                .into_iter(),
        );
        Body {
            kind: quote!({ #wraps #checks #kind }),
            stack,
            depth,
            lead: TokenStream::new(),
            order: joined(mode, part.into_iter()),
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
            order: Order::default(),
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
        .any(|field| matches!(field.key, Key::Int(_) | Key::Label { .. }));
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
            Key::Text(_) | Key::Int(_) | Key::Position | Key::Skipped => None,
        })
        .unzip();
    labels.first().map(|label| {
        quote!(const _: fn(#label) = |__label| match __label { #(#label::#variants)|* => {} };)
    })
}

/// A field's name, for a diagnostic.
fn member_name(field: &Field) -> String {
    field.member.to_token_stream().to_string()
}
