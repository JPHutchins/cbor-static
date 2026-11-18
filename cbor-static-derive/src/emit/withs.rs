//! `#[cbor(with = module)]` (#5 Q19): for each such field, a type of its own per derived direction
//! that calls the module, and the descriptor that calls it.

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::Member;
use syn::ext::IdentExt as _;

use super::params::{Params, codec_generics, params};
use super::{Trait, derives};
use crate::Directions;
use crate::model::{Field, Item};

/// Whose fields the derive emits types of its own for: the owner (the item, or an enum's variant),
/// and the item's parameters as the types name them.
#[derive(Clone, Copy)]
pub struct Site<'a> {
    pub owner: &'a Ident,
    pub named: &'a TokenStream,
}

impl<'a> Site<'a> {
    pub const fn new(owner: &'a Ident, named: &'a TokenStream) -> Self {
        Self { owner, named }
    }
}

/// The type of its own that a `with` field's module is called through under `which` trait, as
/// [`withs`] defines it.
pub fn via(field: &Field, site: Site, which: Trait) -> Option<TokenStream> {
    field.with_module.as_ref().map(|_| {
        let (marker, named) = (with_marker(site.owner, &field.member, which), site.named);
        quote!(#marker #named)
    })
}

/// For each of `owner`'s fields with `with = module`, in each direction `directions` derives, a
/// type whose `WithEncode` or `WithDecode` calls the module's `encode` or `decode` (#5 Q19).
pub fn withs(item: &Item, owner: &Ident, fields: &[Field], directions: Directions) -> TokenStream {
    let Params {
        declared,
        named,
        bounds,
        marker,
    } = params(item);
    let encode = codec_generics(item, Trait::Encode);
    let decode = codec_generics(item, Trait::Decode);
    let ((encode_generics, _, encode_bounds), (decode_generics, _, decode_bounds)) =
        (encode.split_for_impl(), decode.split_for_impl());
    fields
        .iter()
        .filter_map(|field| Some((field, field.with_module.as_ref()?)))
        .flat_map(|(field, module)| {
            [Trait::Encode, Trait::Decode]
                .into_iter()
                .filter(|which| derives(directions, *which))
                .map(move |which| (field, module, which))
        })
        .map(|(field, module, which)| {
            let (name, ty) = (with_marker(owner, &field.member, which), &field.ty);
            let called = match which {
                Trait::Encode => quote! {
                    impl #encode_generics __p::WithEncode for #name #named #encode_bounds {
                        type Value = #ty;
                        fn encode(
                            value: &#ty,
                            writer: &mut ::cbor_static::TokenWriter<'_>,
                        ) -> ::core::result::Result<(), ::cbor_static::Error> {
                            #module::encode(value, writer)
                        }
                    }
                },
                Trait::Decode => quote! {
                    impl #decode_generics __p::WithDecode<'de> for #name #named #decode_bounds {
                        type Value = #ty;
                        fn decode(
                            tokens: &mut ::cbor_static::Tokens<'de>,
                        ) -> ::core::result::Result<#ty, ::cbor_static::Error> {
                            #module::decode(tokens)
                        }
                    }
                },
            };
            quote! {
                struct #name #declared #bounds { #marker }
                #called
            }
        })
        .collect()
}

fn with_marker(owner: &Ident, member: &Member, which: Trait) -> Ident {
    let direction = match which {
        Trait::Encode => "Encode",
        Trait::Decode => "Decode",
    };
    let member = match member {
        Member::Named(ident) => ident.unraw().to_string(),
        Member::Unnamed(index) => index.index.to_string(),
    };
    format_ident!("__{owner}With{direction}_{member}")
}
