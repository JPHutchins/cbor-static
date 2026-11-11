mod enums;

use proc_macro2::{Literal, Span, TokenStream};
use quote::quote;
use syn::{LitByteStr, Type};

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
    let layout = layout(fields.iter().map(|field| {
        let member = &field.member;
        (
            field.name.as_str(),
            quote!(::core::mem::offset_of!(#static_ty, #member)),
            field.omit_if_absent,
        )
    }));
    let types: Vec<&Type> = fields.iter().map(|field| &field.ty).collect();
    let encode = impls(item, directions, Trait::Encode, |path| {
        let (stack, depth) = level(types.iter().map(|ty| child(ty, path)));
        let kinds = types.iter().map(|ty| quote!(<#ty as #path>::ENCODE));
        (
            quote!(__p::EncKind::Struct { layout: LAYOUT, kinds: &[#(#kinds),*], field_ptr: None }),
            stack,
            depth,
        )
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        let (stack, depth) = level(types.iter().map(|ty| child(ty, path)));
        let kinds = types.iter().map(|ty| quote!(<#ty as #path>::DECODE));
        (
            quote!(__p::DecKind::Struct { layout: LAYOUT, kinds: &[#(#kinds),*], drop: __p::drop_fn::<Self>() }),
            stack,
            depth,
        )
    });
    quote! {
        const LAYOUT: &__p::Layout = #layout;
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
                impl #generics ::cbor_static::Encode for #ty {
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
                impl #generics ::cbor_static::Decode<'de> for #ty {
                    const DECODE: &'static __p::DecKind = &#kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                }
            }
        }
        _ => TokenStream::new(),
    }
}

/// `&Layout` from (name, byte offset, omit-if-absent) per entry; names are pre-encoded text items.
fn layout<'a>(entries: impl Iterator<Item = (&'a str, TokenStream, bool)>) -> TokenStream {
    let (blob, fields) = entries.fold((Vec::new(), Vec::new()), |(mut blob, mut fields), (name, offset, omit)| {
		let len = u8::try_from(name.len()).unwrap_or(u8::MAX);
		blob.extend(if len < 24 { vec![0x60 | len] } else { vec![0x78, len] });
		let at = Literal::u16_unsuffixed(u16::try_from(blob.len()).unwrap_or(u16::MAX));
		blob.extend(name.as_bytes());
		let len = Literal::u8_unsuffixed(len);
		fields.push(quote! {
			__p::FieldLayout { offset: __p::offset(#offset), name_at: #at, name_len: #len, omit_if_absent: #omit }
		});
		(blob, fields)
	});
    let blob = LitByteStr::new(&blob, Span::call_site());
    quote!(&__p::Layout { names: #blob, fields: &[#(#fields),*] })
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
