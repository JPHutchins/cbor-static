mod enums;

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::Type;

use crate::Directions;
use crate::model::{Field, Item, Shape};

#[derive(Clone, Copy)]
enum Trait {
    Encode,
    Decode,
}

pub fn item(item: &Item, directions: Directions) -> TokenStream {
    let body = match &item.shape {
        Shape::Struct(fields) => structure(item, fields, directions),
        Shape::Enum { variants, repr } => enums::enumeration(item, variants, *repr, directions),
    };
    quote! {
        #[allow(non_camel_case_types, dead_code, clippy::all, clippy::pedantic)]
        const _: () = {
            use ::cbor_static::__private as __p;
            #body
        };
    }
}

fn structure(item: &Item, fields: &[Field], directions: Directions) -> TokenStream {
    let static_ty = static_type(item);
    let layout = layout(
        &format_ident!("LAYOUT"),
        fields.iter().map(|field| {
            let member = &field.member;
            (
                text_key(&field.name),
                quote!(::core::mem::offset_of!(#static_ty, #member)),
                field.omit_if_absent,
            )
        }),
    );
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
            quote!(__p::DecKind::Struct { ops: &__p::DECODE_MAP, layout: LAYOUT, kinds: &[#(#kinds),*], drop: __p::drop_fn::<Self>() }),
            stack,
            depth,
        )
    });
    quote! {
        #layout
        #encode
        #decode
    }
}

/// The `Encode` or `Decode` impl, when `directions` asks for it, around the kind, stack and
/// depth that `body` builds from the trait path.
fn impls(
    item: &Item,
    directions: Directions,
    which: Trait,
    body: impl FnOnce(&TokenStream) -> (TokenStream, TokenStream, TokenStream),
) -> TokenStream {
    let ident = &item.ident;
    let ty = item
        .lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |lifetime| quote!(#ident<#lifetime>));
    match (which, directions) {
        (Trait::Encode, Directions::Encode | Directions::Both) => {
            let generics = item.lifetime.as_ref().map(|lifetime| quote!(<#lifetime>));
            let (kind, stack, depth) = body(&quote!(::cbor_static::Encode));
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Encode for #ty {
                    const ENCODE: &'static __p::EncKind = &#kind;
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
            let (kind, stack, depth) = body(&quote!(::cbor_static::Decode<'de>));
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Decode<'de> for #ty {
                    const DECODE: &'static __p::DecKind = &#kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
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

fn child(ty: &Type, path: &TokenStream) -> (TokenStream, TokenStream) {
    (
        quote!(<#ty as #path>::Stack),
        quote!(<#ty as #path>::MAX_DEPTH),
    )
}

fn static_type(item: &Item) -> TokenStream {
    let ident = &item.ident;
    item.lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |_| quote!(#ident<'static>))
}

#[cfg(test)]
mod tests;
