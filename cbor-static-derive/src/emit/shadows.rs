//! The types an enum's payloads decode through, and where each payload field lies in the enum: the
//! shadow structs, the `repr(C, u8)` mirror, and their offsets or the enum's own (D-21, R-129).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Member};

use super::makers;
use crate::model::{Field, Offsets, Payload, Repr, Variant};

/// Per variant: the shadow (or mirror) type the payload is decoded through, if any, and the types
/// its fields' `default = "path"` need.
pub struct Shadow {
    pub ident: Ident,
    pub definition: TokenStream,
}

pub fn shadow(
    ident: &Ident,
    variant: &Variant,
    repr: Repr,
    generics: Option<&TokenStream>,
    marker: Option<&TokenStream>,
) -> Option<Shadow> {
    let name = format_ident!("__{}{}", ident, variant.ident);
    let (attrs, tag) = match repr {
        Repr::Rust => (quote!(), quote!()),
        Repr::U8 => (quote!(#[repr(C)]), quote!(pub tag: u8,)),
        Repr::CU8 => (quote!(#[repr(C)]), quote!()),
    };
    let definition = match &variant.payload {
        Payload::Unit => return None,
        Payload::Newtype(field) if repr == Repr::Rust => {
            let ty = &field.ty;
            let definition = quote!(pub type #name #generics = #ty;);
            return Some(Shadow {
                ident: name,
                definition,
            });
        }
        Payload::Newtype(field) => {
            let ty = &field.ty;
            quote!(#attrs pub struct #name #generics { #tag pub f0: #ty, #marker })
        }
        Payload::Struct(fields) => {
            let makers = makers(&owner(ident, variant), fields, generics, marker);
            let fields = fields
                .iter()
                .map(|Field { member, ty, .. }| quote!(pub #member: #ty,));
            quote! {
                #attrs pub struct #name #generics { #tag #(#fields)* #marker }
                #makers
            }
        }
    };
    Some(Shadow {
        ident: name,
        definition,
    })
}

/// The `repr(C, u8)` enum's mirror: its tag, then a union of every variant's shadow, asserted to be
/// the enum's size (RFC 2195).
pub fn mirror(
    ident: &Ident,
    shadows: &[Option<Shadow>],
    generics: Option<&TokenStream>,
    statics: Option<&TokenStream>,
) -> TokenStream {
    let (mirror, payload_union) = (
        format_ident!("__{}Mirror", ident),
        format_ident!("__{}Payload", ident),
    );
    let arms = shadows.iter().flatten().map(|s| {
        let shadow = &s.ident;
        let field = format_ident!("{}", shadow.to_string().to_lowercase());
        quote!(pub #field: ::core::mem::ManuallyDrop<#shadow #generics>,)
    });
    quote! {
        #[repr(C)] pub union #payload_union #generics { #(#arms)* pub __unit: () }
        #[repr(C)] pub struct #mirror #generics { pub tag: u8, pub payload: #payload_union #generics }
        const _: () = assert!(::core::mem::size_of::<#mirror #statics>() == ::core::mem::size_of::<#ident #statics>());
    }
}

/// How the enum `ident` places its payload fields.
pub struct Placement<'a> {
    pub ident: &'a Ident,
    pub statics: Option<&'a TokenStream>,
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
                let mirror = format_ident!("__{}Mirror", ident);
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
    format_ident!("{}{}", item, variant.ident)
}
