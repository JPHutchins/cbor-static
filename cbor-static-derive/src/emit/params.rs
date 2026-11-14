//! How a generic item's parameters are written into its impls and helper types (R-127).

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::token::{Comma, Where};
use syn::{GenericParam, Generics, WhereClause, WherePredicate, parse_quote};

use super::Trait;
use crate::model::Item;

/// How the item's parameters are written (R-127): as an impl or a helper type declares them, as
/// the type names them, its where clause, and a marker field that uses every one of them.
pub struct Params {
    pub declared: TokenStream,
    pub named: TokenStream,
    pub bounds: TokenStream,
    pub marker: Option<TokenStream>,
}

pub fn params(item: &Item) -> Params {
    let ident = &item.ident;
    let (declared, named, bounds) = item.generics.split_for_impl();
    Params {
        declared: declared.to_token_stream(),
        named: named.to_token_stream(),
        bounds: bounds.to_token_stream(),
        marker: (!item.generics.params.is_empty())
            .then(|| quote!(pub __marker: ::core::marker::PhantomData<fn() -> #ident #named>,)),
    }
}

/// The item's parameters as `which`'s impl declares them: each type parameter bounded by the trait,
/// as serde bounds them, and for `Decode`, the input lifetime `'de`, outliving the item's own.
pub fn codec_generics(item: &Item, which: Trait) -> Generics {
    let (path, de): (TokenStream, Option<GenericParam>) = match which {
        Trait::Encode => (quote!(::cbor_static::Encode), None),
        Trait::Decode => (
            quote!(::cbor_static::Decode<'de>),
            Some(item.lifetime.as_ref().map_or_else(
                || parse_quote!('de),
                |lifetime| parse_quote!('de: #lifetime),
            )),
        ),
    };
    let predicates: Punctuated<WherePredicate, Comma> = item
        .generics
        .where_clause
        .iter()
        .flat_map(|clause| clause.predicates.iter().cloned())
        .chain(item.generics.type_params().map(|param| {
            let ident = &param.ident;
            parse_quote!(#ident: #path)
        }))
        .collect();
    Generics {
        params: de
            .into_iter()
            .chain(item.generics.params.iter().cloned())
            .collect(),
        where_clause: (!predicates.is_empty()).then(|| WhereClause {
            where_token: Where::default(),
            predicates,
        }),
        ..item.generics.clone()
    }
}

/// Whether the item has type or const parameters, which its field offsets depend on, so that its
/// layout is built inside each impl rather than once.
pub fn generic(item: &Item) -> bool {
    item.generics.type_params().next().is_some() || item.generics.const_params().next().is_some()
}
