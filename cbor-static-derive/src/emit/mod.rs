mod enums;
mod forms;
mod structs;

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};

use crate::Directions;
use crate::model::{Item, Key, Shape};

#[derive(Clone, Copy)]
enum Trait {
    Encode,
    Decode,
}

pub fn item(item: &Item, directions: Directions) -> TokenStream {
    let body = match &item.shape {
        Shape::Struct { fields, form } => structs::structure(item, fields, *form, directions),
        Shape::Unit => structs::transparent(item, None, directions),
        Shape::Newtype(field) => structs::transparent(item, Some(field.as_ref()), directions),
        Shape::Enum {
            variants,
            repr,
            form,
        } => enums::enumeration(item, variants, *repr, form, directions),
    };
    quote! {
        #[allow(non_camel_case_types, non_upper_case_globals, dead_code, clippy::all, clippy::pedantic)]
        const _: () = {
            use ::cbor_static::__private as __p;
            #body
        };
    }
}

/// A codec trait impl's items: `kind` is the descriptor's `&'static` expression, and `lead` is
/// `Decode`'s alone.
struct Body {
    kind: TokenStream,
    stack: TokenStream,
    depth: TokenStream,
    lead: TokenStream,
}

/// The `Encode` or `Decode` impl, when `directions` asks for it, around the body built from the
/// trait path.
fn impls(
    item: &Item,
    directions: Directions,
    which: Trait,
    body: impl FnOnce(&TokenStream) -> Body,
) -> TokenStream {
    let ident = &item.ident;
    let ty = item
        .lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |lifetime| quote!(#ident<#lifetime>));
    match (which, directions) {
        (Trait::Encode, Directions::Encode | Directions::Both) => {
            let generics = item.lifetime.as_ref().map(|lifetime| quote!(<#lifetime>));
            let Body {
                kind, stack, depth, ..
            } = body(&quote!(::cbor_static::Encode));
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Encode for #ty {
                    const ENCODE: &'static __p::EncKind = #kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                }
            }
        }
        (Trait::Decode, Directions::Decode | Directions::Both) => {
            let generics = item.lifetime.as_ref().map_or_else(
                || quote!(<'de>),
                |lifetime| quote!(<'de: #lifetime, #lifetime>),
            );
            let Body {
                kind,
                stack,
                depth,
                lead,
            } = body(&quote!(::cbor_static::Decode<'de>));
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Decode<'de> for #ty {
                    const DECODE: &'static __p::DecKind = #kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                    type OptionStack = __p::Level<Self::Stack>;
                    const LEAD: __p::Lead = #lead;
                }
            }
        }
        _ => TokenStream::new(),
    }
}

/// The `name: &Layout` const, with its keys and their pre-encoded names, from (key, byte offset,
/// omit-if-absent) per entry.
fn layout(
    name: &Ident,
    entries: impl Iterator<Item = (TokenStream, TokenStream, bool)>,
) -> TokenStream {
    let keys = format_ident!("{}_KEYS", name);
    let names = format_ident!("{}_NAMES", name);
    let (entry_keys, fields): (Vec<_>, Vec<_>) = entries
        .enumerate()
        .map(|(index, (key, offset, omit))| {
            (key, quote!(__p::field(#keys, #index, #offset, #omit)))
        })
        .unzip();
    quote! {
        const #keys: &[__p::Key] = &[#(#entry_keys),*];
        const #names: [u8; __p::names_len(#keys)] = __p::names(#keys);
        const #name: &__p::Layout = &__p::Layout { names: &#names, fields: &[#(#fields),*] };
    }
}

fn text_key(name: &str) -> TokenStream {
    quote!(__p::Key::Text(#name))
}

fn key(key: &Key) -> TokenStream {
    match key {
        Key::Text(text) => text_key(text),
        Key::Int(int) => quote!(__p::Key::Int(#int)),
        Key::Label { label, variant } => quote!(__p::Key::Int(#label::#variant as i64)),
        Key::Position => quote!(__p::Key::None),
    }
}

/// One stack level over `children`: `(Level<Widest<…>>, 1 + deeper(…))`.
fn level(
    children: impl DoubleEndedIterator<Item = (TokenStream, TokenStream)>,
) -> (TokenStream, TokenStream) {
    let (widest, deepest) = children.rev().fold(
        (quote!(()), quote!(0)),
        |(widest, deepest), (stack, depth)| {
            (
                quote!(__p::Widest<#stack, #widest>),
                quote!(__p::deeper(#depth, #deepest)),
            )
        },
    );
    (quote!(__p::Level<#widest>), quote!(1 + #deepest))
}

fn child(ty: &impl ToTokens, path: &TokenStream) -> (TokenStream, TokenStream) {
    (
        quote!(<#ty as #path>::Stack),
        quote!(<#ty as #path>::MAX_DEPTH),
    )
}

/// A field's stack and depth: a `#[cbor(bytes)]` field's byte string opens no frame.
fn field_child(ty: &impl ToTokens, bytes: bool, path: &TokenStream) -> (TokenStream, TokenStream) {
    if bytes {
        (quote!(()), quote!(0))
    } else {
        child(ty, path)
    }
}

/// A field's descriptor under `which` trait at `path`: its type's own, or with `#[cbor(bytes)]`,
/// its byte string's.
fn field_kind(ty: &impl ToTokens, bytes: bool, which: Trait, path: &TokenStream) -> TokenStream {
    match (which, bytes) {
        (Trait::Encode, false) => quote!(<#ty as #path>::ENCODE),
        (Trait::Decode, false) => quote!(<#ty as #path>::DECODE),
        (Trait::Encode, true) => quote!(<#ty as ::cbor_static::ByteString>::ENCODE_BYTES),
        (Trait::Decode, true) => quote!(<#ty as ::cbor_static::ByteString>::DECODE_BYTES),
    }
}

fn static_type(item: &Item) -> TokenStream {
    let ident = &item.ident;
    item.lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |_| quote!(#ident<'static>))
}

#[cfg(test)]
mod tests;
