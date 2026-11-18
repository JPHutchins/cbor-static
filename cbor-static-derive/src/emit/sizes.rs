//! `#[cbor(max_encoded_len)]` (#5 Q2): the item's `MaxEncodedLen`, summed in const evaluation from
//! its fields' own bounds, its keys and its heads. An unbounded field fails to compile (E0277) at
//! the field's type.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned as _;

use super::params::bounded;
use super::{Trait, derives, forms, key};
use crate::Directions;
use crate::model::{EnumForm, Field, Item, Key, Payload, Shape, StructForm, Variant};

pub fn max_encoded_len(item: &Item, directions: Directions) -> TokenStream {
    if !(item.max_encoded_len && derives(directions, Trait::Encode)) {
        return TokenStream::new();
    }
    let len = match &item.shape {
        Shape::Struct {
            fields,
            form: StructForm::Map { .. },
            ..
        } => map(fields, &[]),
        Shape::Struct {
            fields,
            form: StructForm::Array,
            ..
        } => array(fields),
        Shape::Unit => null(),
        Shape::Newtype { field, .. } => value(field),
        Shape::Enum { variants, form, .. } => largest(
            variants
                .iter()
                .zip(forms::variant_keys(form, variants))
                .map(|(variant, name)| variant_len(form, variant, &quote!(__p::key_len(&#name)))),
        ),
    };
    let ident = &item.ident;
    let generics = bounded(
        item,
        &quote!(::cbor_static::MaxEncodedLen + ::cbor_static::Encode),
        None,
    );
    let (generics, named, bounds) = generics.split_for_impl();
    quote! {
        #[automatically_derived]
        impl #generics ::cbor_static::MaxEncodedLen for #ident #named #bounds {
            const MAX_ENCODED_LEN: usize = #len;
        }
    }
}

/// The largest of `lens`, nested two at a time.
fn largest(lens: impl Iterator<Item = TokenStream>) -> TokenStream {
    lens.reduce(|a, b| quote!(__p::larger(#a, #b)))
        .unwrap_or_else(null)
}

/// A variant's longest encoding under `form`, whose encoded identifier is `name`.
fn variant_len(form: &EnumForm, variant: &Variant, name: &TokenStream) -> TokenStream {
    let payload = match &variant.payload {
        Payload::Unit => None,
        Payload::Newtype(field) => Some(value(field)),
        Payload::Struct(fields) => Some(map(fields, &[])),
    };
    match (form, payload) {
        (EnumForm::External | EnumForm::Index | EnumForm::CborTag, None) => name.clone(),
        (EnumForm::External | EnumForm::Index, Some(payload)) => {
            quote!(__p::head_len(1) + #name + #payload)
        }
        (EnumForm::CborTag, Some(payload)) => quote!(#name + #payload),
        (EnumForm::Untagged, None) => null(),
        (EnumForm::Untagged, Some(payload)) => payload,
        (EnumForm::Adjacent { tag, content }, payload) => {
            let tagged = quote!(__p::key_len(&__p::Key::Text(#tag)) + #name);
            payload.map_or_else(
                || quote!(__p::head_len(1) + #tagged),
                |payload| {
                    quote!(__p::head_len(2) + #tagged + __p::key_len(&__p::Key::Text(#content)) + #payload)
                },
            )
        }
        (EnumForm::Internal { tag }, _) => {
            let tagged = quote!(__p::key_len(&__p::Key::Text(#tag)) + #name);
            match &variant.payload {
                Payload::Unit => quote!(__p::head_len(1) + #tagged),
                Payload::Struct(fields) => map(fields, &[tagged]),
                Payload::Newtype(field) => {
                    let ty = &field.ty;
                    let (count, entries) = flattened(&quote!(#ty), field);
                    quote!(__p::head_len((1 + #count) as u64) + #tagged + #entries)
                }
            }
        }
    }
}

/// A map of `fields`' entries after `leading` ones, each given as its encoded key and value
/// together.
fn map(fields: &[Field], leading: &[TokenStream]) -> TokenStream {
    let (counts, entries): (Vec<_>, Vec<_>) = fields.iter().filter_map(entry).unzip();
    let count = leading.len();
    quote!(__p::head_len((#count #(+ #counts)*) as u64) #(+ #leading)* #(+ #entries)*)
}

/// A field's entries in its struct's map: how many, and their longest encoding.
fn entry(field: &Field) -> Option<(TokenStream, TokenStream)> {
    let ty = &field.ty;
    match &field.key {
        Key::Skipped | Key::Position => None,
        Key::Other => Some((quote!(0), bound(&quote!(#ty), field))),
        Key::Flattened => Some(flattened(&quote!(#ty), field)),
        named @ (Key::Text(_) | Key::Int(_) | Key::Label { .. }) => {
            let key = key(named);
            let value = value(field);
            Some((quote!(1), quote!(__p::key_len(&#key) + #value)))
        }
    }
}

/// A map-form type's entries joined to another map: as many as its kinds, and its bound but for
/// its own head, which is at least one byte.
fn flattened(ty: &TokenStream, field: &Field) -> (TokenStream, TokenStream) {
    let own = bound(ty, field);
    (
        quote!(__p::EncKind::kinds(<#ty as ::cbor_static::Encode>::ENCODE).len()),
        quote!((#own - 1)),
    )
}

fn array(fields: &[Field]) -> TokenStream {
    let written: Vec<_> = fields
        .iter()
        .filter(|field| !matches!(field.key, Key::Skipped))
        .map(value)
        .collect();
    let count = written.len();
    quote!(__p::head_len(#count as u64) #(+ #written)*)
}

fn null() -> TokenStream {
    quote!(<() as ::cbor_static::MaxEncodedLen>::MAX_ENCODED_LEN)
}

/// The longest encoding of a field's value: its type's bound, or under `bytes` or `max_len`, that
/// of the string or container they bound.
fn value(field: &Field) -> TokenStream {
    let ty = &field.ty;
    match (field.bytes, &field.max_len) {
        (true, Some(max_len)) => quote!(__p::container_len({ #max_len }, 1)),
        (true, None) => {
            let capacity = quote_spanned!(field.ty.span()=> <#ty as __p::ByteCapacity>::CAPACITY);
            quote!(__p::container_len(#capacity, 1))
        }
        (false, Some(max_len)) => bound(&quote!(__p::Limited<#ty, { #max_len }>), field),
        (false, None) => bound(&quote!(#ty), field),
    }
}

/// `ty`'s `MaxEncodedLen`, which rustc reports missing at `field`'s type.
fn bound(ty: &TokenStream, field: &Field) -> TokenStream {
    quote_spanned!(field.ty.span()=> <#ty as ::cbor_static::MaxEncodedLen>::MAX_ENCODED_LEN)
}
