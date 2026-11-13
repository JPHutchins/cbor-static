//! What each enum representation (D-10) changes in the derive output: its ops, how a variant is
//! identified in the layout, the layout's extra entries, and the classes of initial byte its
//! values start with.

use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::Type;

use super::{Trait, text_key};
use crate::model::{EnumForm, Payload, Variant};

pub struct Ops {
    pub encode: TokenStream,
    pub decode: TokenStream,
}

pub fn ops(form: &EnumForm) -> Ops {
    let (encode, decode) = match form {
        EnumForm::External => (quote!(__p::ENCODE_EXTERNAL), quote!(__p::DECODE_EXTERNAL)),
        EnumForm::Untagged => (quote!(__p::ENCODE_UNTAGGED), quote!(__p::DECODE_UNTAGGED)),
        EnumForm::Index => (quote!(__p::ENCODE_INDEX), quote!(__p::DECODE_INDEX)),
        EnumForm::CborTag => (quote!(__p::ENCODE_CBOR_TAG), quote!(__p::DECODE_CBOR_TAG)),
        EnumForm::Internal { .. } => (quote!(__p::ENCODE_INTERNAL), quote!(__p::DECODE_INTERNAL)),
        EnumForm::Adjacent { .. } => (quote!(__p::ENCODE_ADJACENT), quote!(__p::DECODE_ADJACENT)),
    };
    Ops { encode, decode }
}

/// How the layout identifies each variant. An untagged variant's classes depend on its payload
/// type, so the layout holds a placeholder that only the decoder's own layout fills in.
pub fn variant_keys(form: &EnumForm, variants: &[Variant]) -> Vec<TokenStream> {
    match form {
        EnumForm::Index => discriminants(variants)
            .into_iter()
            .map(|discriminant| quote!(__p::Key::Int(#discriminant)))
            .collect(),
        EnumForm::CborTag => variants
            .iter()
            .filter_map(|variant| variant.cbor_tag)
            .map(|tag| quote!(__p::Key::Tag(#tag)))
            .collect(),
        EnumForm::Untagged => variants
            .iter()
            .map(|_| quote!(__p::Key::Lead(__p::Lead::NONE)))
            .collect(),
        EnumForm::External | EnumForm::Internal { .. } | EnumForm::Adjacent { .. } => variants
            .iter()
            .map(|variant| text_key(&variant.name))
            .collect(),
    }
}

/// Each variant's discriminant as Rust assigns it: the one written, or one more than the one
/// before, starting from 0 (D-10a). Each is the last one written plus a count, so the tokens grow
/// linearly with the variants.
fn discriminants(variants: &[Variant]) -> Vec<TokenStream> {
    variants
        .iter()
        .scan(None, |last: &mut Option<(TokenStream, i64)>, variant| {
            let (base, after) = match (&variant.discriminant, last.take()) {
                (Some(written), _) => (quote!((#written) as i64), 0),
                (None, Some((base, after))) => (base, after + 1),
                (None, None) => (quote!(0), 0),
            };
            let offset = Literal::i64_unsuffixed(after);
            let discriminant = quote!(#base + #offset);
            *last = Some((base, after));
            Some(discriminant)
        })
        .collect()
}

/// Entries after the variants: the tag and content keys of internally and adjacently tagged enums.
pub fn extras(form: &EnumForm) -> Vec<TokenStream> {
    match form {
        EnumForm::Internal { tag } => vec![text_key(tag)],
        EnumForm::Adjacent { tag, content } => vec![text_key(tag), text_key(content)],
        EnumForm::External | EnumForm::Index | EnumForm::CborTag | EnumForm::Untagged => vec![],
    }
}

/// What a newtype variant's payload type must be under `form`, checked at compile time for the
/// direction its impl derives (`Encode` or `Decode`, at `path`): an internally tagged payload joins
/// the tag entry to its own entries, so it must be a map-form struct, none of whose keys is the tag.
pub fn newtype_check(
    form: &EnumForm,
    ty: &Type,
    which: Trait,
    path: &TokenStream,
) -> Option<TokenStream> {
    let EnumForm::Internal { tag } = form else {
        return None;
    };
    let (direction, kind) = match which {
        Trait::Encode => (
            quote!(Encoding),
            quote!(__p::EncKind::layout(<#ty as #path>::ENCODE)),
        ),
        Trait::Decode => (
            quote!(Decoding),
            quote!(__p::DecKind::layout(<#ty as #path>::DECODE)),
        ),
    };
    let message =
        format!("cbor-static: a field of the payload is keyed like the enum's tag, `{tag}`");
    Some(quote! {
        __p::map_form::<#ty, __p::#direction>();
        assert!(!__p::names_text(#kind, #tag), #message);
    })
}

/// The classes of initial byte the enum's values start with.
pub fn lead(form: &EnumForm, variants: &[Variant], path: &TokenStream) -> TokenStream {
    match form {
        EnumForm::External => union(variants.iter().map(|variant| match variant.payload {
            Payload::Unit => quote!(__p::Lead::TEXT),
            Payload::Newtype(_) | Payload::Struct(_) => quote!(__p::Lead::MAP),
        })),
        EnumForm::Index => union(variants.iter().map(|variant| match variant.payload {
            Payload::Unit => quote!(__p::Lead::UINT.union(__p::Lead::NINT)),
            Payload::Newtype(_) | Payload::Struct(_) => quote!(__p::Lead::ARRAY),
        })),
        EnumForm::CborTag => union(
            variants
                .iter()
                .filter_map(|variant| variant.cbor_tag)
                .map(|tag| quote!(__p::Lead::tagged(#tag))),
        ),
        EnumForm::Internal { .. } | EnumForm::Adjacent { .. } => quote!(__p::Lead::MAP),
        EnumForm::Untagged => union(variants.iter().map(|variant| variant_lead(variant, path))),
    }
}

/// The layout the decoder walks: `LAYOUT`, or for an untagged enum the same placements with each
/// variant's classes, behind asserts that no two variants share one (D-10).
pub fn decode_layout(form: &EnumForm, variants: &[Variant], path: &TokenStream) -> TokenStream {
    if !matches!(form, EnumForm::Untagged) {
        return quote!(LAYOUT);
    }
    let leads: Vec<TokenStream> = variants
        .iter()
        .map(|variant| variant_lead(variant, path))
        .collect();
    let disjoint =
        variants
            .iter()
            .zip(&leads)
            .enumerate()
            .flat_map(|(at, (a, a_lead))| {
                variants.iter().zip(&leads).skip(at + 1).map(move |(b, b_lead)| {
            let message = format!(
                "cbor-static: untagged variants `{}` and `{}` can start with the same item",
                a.ident, b.ident
            );
            quote!(assert!(!#a_lead.overlaps(#b_lead), #message);)
        })
            });
    let count = Literal::usize_unsuffixed(variants.len());
    quote!({
        #(#disjoint)*
        &__p::Layout { names: &const { __p::names::<{ __p::Lead::ENCODED_LEN * #count }>(&[#(__p::Key::Lead(#leads)),*]) }, fields: LAYOUT.fields }
    })
}

/// A unit variant is null; a struct variant is a map.
fn variant_lead(variant: &Variant, path: &TokenStream) -> TokenStream {
    match &variant.payload {
        Payload::Unit => quote!(__p::Lead::NULL),
        Payload::Newtype(field) if field.bytes => quote!(__p::Lead::BYTES),
        Payload::Newtype(field) => {
            let ty = &field.ty;
            quote!(<#ty as #path>::LEAD)
        }
        Payload::Struct(_) => quote!(__p::Lead::MAP),
    }
}

fn union(leads: impl Iterator<Item = TokenStream>) -> TokenStream {
    leads.fold(
        quote!(__p::Lead::NONE),
        |all, lead| quote!(#all.union(#lead)),
    )
}
