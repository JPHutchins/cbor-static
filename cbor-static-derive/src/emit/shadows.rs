//! The types an enum's payloads decode through, and where each payload field lies in the enum: the
//! shadow structs, the `repr(C, u8)` mirror, and their offsets or the enum's own (D-21, R-129).

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Ident, Member};

use super::Params;
use crate::model::{Field, Offsets, Payload, Repr, Variant};

/// Per variant: the shadow (or mirror) type the payload is decoded through, if any, as it is named
/// in `ty`, and the types its fields' `default = "path"` need. A Rust-repr newtype's is its field's
/// type.
pub struct Shadow {
    pub ident: Ident,
    pub ty: TokenStream,
    pub definition: TokenStream,
}

pub fn shadow(ident: &Ident, variant: &Variant, repr: Repr, params: &Params) -> Option<Shadow> {
    let Params {
        declared,
        named,
        bounds,
        marker,
    } = params;
    let name = format_ident!("__{}{}", ident, variant.ident);
    let (attrs, tag) = match repr {
        Repr::Rust => (quote!(), quote!()),
        Repr::U8 => (quote!(#[repr(C)]), quote!(pub tag: u8,)),
        Repr::CU8 => (quote!(#[repr(C)]), quote!()),
    };
    let definition = match &variant.payload {
        Payload::Unit => return None,
        Payload::Newtype(field) if repr == Repr::Rust => {
            return Some(Shadow {
                ident: name,
                ty: field.ty.to_token_stream(),
                definition: TokenStream::new(),
            });
        }
        Payload::Newtype(field) => {
            let ty = &field.ty;
            quote!(#attrs pub struct #name #declared #bounds { #tag pub f0: #ty, #marker })
        }
        Payload::Struct(fields) => {
            use super::makers;

            let makers = makers(&owner(ident, variant), fields, params);
            let fields = fields
                .iter()
                .map(|Field { member, ty, .. }| quote!(pub #member: #ty,));
            quote! {
                #attrs pub struct #name #declared #bounds { #tag #(#fields)* #marker }
                #makers
            }
        }
    };
    Some(Shadow {
        ty: quote!(#name #named),
        ident: name,
        definition,
    })
}

/// The `repr(C, u8)` enum's mirror: its tag, then a union of every variant's shadow (RFC 2195).
pub fn mirror(ident: &Ident, shadows: &[Option<Shadow>], params: &Params) -> TokenStream {
    let Params {
        declared,
        named,
        bounds,
        marker,
    } = params;
    let (mirror, payload_union) = (
        mirror_ident(ident),
        format_ident!("__{ident}__Payload", ident = ident),
    );
    let arms = shadows.iter().flatten().map(|s| {
        let (field, ty) = (
            format_ident!("{field}", field = s.ident.to_string().to_lowercase()),
            &s.ty,
        );
        quote!(pub #field: ::core::mem::ManuallyDrop<#ty>,)
    });
    let marker = marker
        .as_ref()
        .map(|_| quote!(pub __marker: ::core::mem::ManuallyDrop<::core::marker::PhantomData<fn() -> #ident #named>>,));
    quote! {
        #[repr(C)] pub union #payload_union #declared #bounds { #(#arms)* #marker pub __unit: () }
        #[repr(C)] pub struct #mirror #declared #bounds { pub tag: u8, pub payload: #payload_union #named }
    }
}

/// That the mirror is the enum's size, `placed_in` naming each's parameters.
pub fn mirrored(ident: &Ident, placed_in: &TokenStream) -> TokenStream {
    let mirror = mirror_ident(ident);
    quote!(assert!(::core::mem::size_of::<#mirror #placed_in>() == ::core::mem::size_of::<#ident #placed_in>());)
}

/// The mirror's name, which no variant's shadow, `__{Enum}{Variant}`, can take.
fn mirror_ident(ident: &Ident) -> Ident {
    format_ident!("__{ident}__Mirror", ident = ident)
}

/// How the enum `ident` places its payload fields, `statics` naming its parameters where the
/// offsets are taken.
pub struct Placement<'a> {
    pub ident: &'a Ident,
    pub statics: &'a TokenStream,
    pub repr: Repr,
    pub offsets: Offsets,
}

impl Placement<'_> {
    /// The offset of `variant`'s payload field `member` (none for a newtype's one field), whose
    /// shadow is `shadow`: from the enum's own variant under `offset_of_enum`, else from the
    /// shadow, after the mirror's tag for `repr(C, u8)`.
    pub fn offset(&self, variant: &Ident, shadow: &Ident, member: Option<&Member>) -> TokenStream {
        let Self {
            ident,
            statics,
            repr,
            offsets,
        } = self;
        let (shadowed, own) = member.map_or_else(
            || (quote!(f0), quote!(0)),
            |member| (quote!(#member), quote!(#member)),
        );
        match (offsets, repr) {
            (Offsets::Enum, _) => quote!(::core::mem::offset_of!(#ident #statics, #variant.#own)),
            (Offsets::Shadows, Repr::CU8) => {
                let mirror = mirror_ident(ident);
                quote! {
                    ::core::mem::offset_of!(#mirror #statics, payload) + ::core::mem::offset_of!(#shadow #statics, #shadowed)
                }
            }
            (Offsets::Shadows, Repr::Rust | Repr::U8) => {
                quote!(::core::mem::offset_of!(#shadow #statics, #shadowed))
            }
        }
    }
}

/// The prefix of the types the derive defines for `variant` of the enum `item`.
pub fn owner(item: &Ident, variant: &Variant) -> Ident {
    format_ident!("{item}{variant}", item = item, variant = variant.ident)
}
