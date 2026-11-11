use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::Ident;

use super::{Trait, child, impls, layout, level};
use crate::Directions;
use crate::model::{Field, Item, Payload, Repr, Variant};

/// Per variant: the shadow (or mirror) type the payload is decoded through, if any.
struct Shadow {
    ident: Ident,
    definition: TokenStream,
}

pub fn enumeration(
    item: &Item,
    variants: &[Variant],
    repr: Repr,
    directions: Directions,
) -> TokenStream {
    let ident = &item.ident;
    let generics = item.lifetime.as_ref().map(|lifetime| quote!(<#lifetime>));
    let statics = item.lifetime.as_ref().map(|_| quote!(<'static>));
    let marker = item
        .lifetime
        .as_ref()
        .map(|lifetime| quote!(pub __marker: ::core::marker::PhantomData<&#lifetime ()>,));
    let shadows: Vec<Option<Shadow>> = variants
        .iter()
        .map(|variant| shadow(ident, variant, repr, generics.as_ref(), marker.as_ref()))
        .collect();
    let mirror = format_ident!("__{}Mirror", ident);
    let payload_union = format_ident!("__{}Payload", ident);
    let mirror_definition = (repr == Repr::CU8).then(|| {
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
	});
    let offset_in = |shadow: &Ident, member: &TokenStream| match repr {
        Repr::CU8 => quote! {
            ::core::mem::offset_of!(#mirror #statics, payload) + ::core::mem::offset_of!(#shadow #statics, #member)
        },
        Repr::Rust | Repr::U8 => quote!(::core::mem::offset_of!(#shadow #statics, #member)),
    };
    let variant_layout = layout(variants.iter().zip(&shadows).map(|(variant, shadow)| {
        let offset = shadow
            .as_ref()
            .filter(|_| matches!(variant.payload, Payload::Newtype(_)) && repr != Repr::Rust)
            .map_or_else(|| quote!(0), |s| offset_in(&s.ident, &quote!(f0)));
        (variant.name.as_str(), offset, false)
    }));
    let payload_layouts = variants
        .iter()
        .zip(&shadows)
        .filter_map(|(variant, shadow)| match (&variant.payload, shadow) {
            (Payload::Struct(fields), Some(s)) => {
                let name = layout_ident(&variant.ident);
                let layout = layout(fields.iter().map(|field| {
                    let member = &field.member;
                    (
                        field.name.as_str(),
                        offset_in(&s.ident, &quote!(#member)),
                        field.omit_if_absent,
                    )
                }));
                Some(quote!(const #name: &__p::Layout = #layout;))
            }
            _ => None,
        });
    let definitions = shadows.iter().flatten().map(|s| &s.definition);
    let glue = (repr == Repr::Rust).then(|| glue(item, variants, &shadows));
    let encode = impls(item, directions, Trait::Encode, |path| {
        encode_kind(variants, repr, path)
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        decode_kind(variants, &shadows, repr, generics.as_ref(), path)
    });
    quote! {
        #(#definitions)*
        #mirror_definition
        const LAYOUT: &__p::Layout = #variant_layout;
        #(#payload_layouts)*
        #glue
        #encode
        #decode
    }
}

fn shadow(
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
        Payload::Newtype(ty) if repr == Repr::Rust => {
            let definition = quote!(pub type #name #generics = #ty;);
            return Some(Shadow {
                ident: name,
                definition,
            });
        }
        Payload::Newtype(ty) => {
            quote!(#attrs pub struct #name #generics { #tag pub f0: #ty, #marker })
        }
        Payload::Struct(fields) => {
            let fields = fields
                .iter()
                .map(|Field { member, ty, .. }| quote!(pub #member: #ty,));
            quote!(#attrs pub struct #name #generics { #tag #(#fields)* #marker })
        }
    };
    Some(Shadow {
        ident: name,
        definition,
    })
}

fn glue(item: &Item, variants: &[Variant], shadows: &[Option<Shadow>]) -> TokenStream {
    let ident = &item.ident;
    let generics = item.lifetime.as_ref().map(|lifetime| quote!(<#lifetime>));
    let indices = variants.iter().enumerate().map(|(index, variant)| {
        let index = Literal::u8_unsuffixed(u8::try_from(index).unwrap_or(u8::MAX));
        let name = &variant.ident;
        match &variant.payload {
            Payload::Unit => quote!(Self::#name => #index),
            Payload::Newtype(_) => quote!(Self::#name(..) => #index),
            Payload::Struct(_) => quote!(Self::#name { .. } => #index),
        }
    });
    let pointers = variants.iter().flat_map(|variant| {
		let name = &variant.ident;
		match &variant.payload {
			Payload::Unit => vec![],
			Payload::Newtype(_) => vec![quote!((Self::#name(value), 0) => ::core::ptr::from_ref(value).cast())],
			Payload::Struct(fields) => fields
				.iter()
				.enumerate()
				.map(|(index, Field { member, .. })| {
					let index = Literal::u8_unsuffixed(u8::try_from(index).unwrap_or(u8::MAX));
					quote!((Self::#name { #member: value, .. }, #index) => ::core::ptr::from_ref(value).cast())
				})
				.collect(),
		}
	});
    let assembles = variants
        .iter()
        .zip(shadows)
        .enumerate()
        .map(|(index, (variant, shadow))| {
            let index = Literal::u8_unsuffixed(u8::try_from(index).unwrap_or(u8::MAX));
            let name = &variant.ident;
            let (shadow_ty, build) = match (&variant.payload, shadow) {
                (Payload::Newtype(_), Some(s)) => {
                    let s = &s.ident;
                    (quote!(#s #generics), quote!(Self::#name(shadow)))
                }
                (Payload::Struct(fields), Some(s)) => {
                    let s = &s.ident;
                    let moves = fields
                        .iter()
                        .map(|Field { member, .. }| quote!(#member: shadow.#member));
                    (quote!(#s #generics), quote!(Self::#name { #(#moves),* }))
                }
                _ => (quote!(()), quote!({ let () = shadow; Self::#name })),
            };
            quote! {
                #[automatically_derived]
                impl #generics __p::Assemble<#index> for #ident #generics {
                    type Shadow = #shadow_ty;
                    fn assemble(shadow: Self::Shadow) -> Self { #build }
                }
            }
        });
    quote! {
        #[automatically_derived]
        impl #generics __p::EnumGlue for #ident #generics {
            fn variant_index(&self) -> u8 { match self { #(#indices),* } }
            fn field_ptr(&self, field: u8) -> *const u8 {
                match (self, field) { #(#pointers,)* _ => ::core::ptr::null() }
            }
        }
        #(#assembles)*
    }
}

fn encode_kind(
    variants: &[Variant],
    repr: Repr,
    path: &TokenStream,
) -> (TokenStream, TokenStream, TokenStream) {
    let glue = (repr == Repr::Rust).then(|| quote!(Some(__p::field_ptr_of::<Self>)));
    let field_ptr = glue.clone().unwrap_or_else(|| quote!(None));
    let index = glue
        .as_ref()
        .map_or_else(|| quote!(__p::index_tag), |_| quote!(__p::index_of::<Self>));
    let payloads = variants.iter().map(|variant| match &variant.payload {
		Payload::Unit => quote!(None),
		Payload::Newtype(ty) => quote!(Some(<#ty as #path>::ENCODE)),
		Payload::Struct(fields) => {
			let name = layout_ident(&variant.ident);
			let kinds = fields.iter().map(|Field { ty, .. }| quote!(<#ty as #path>::ENCODE));
			quote!(Some(&__p::EncKind::Struct { ops: &__p::ENCODE_MAP, layout: #name, kinds: &[#(#kinds),*], field_ptr: #field_ptr }))
		}
	});
    let (stack, depth) = stack(variants, path);
    (
        quote!(__p::EncKind::Enum { ops: &__p::ENCODE_EXTERNAL, layout: LAYOUT, payloads: &[#(#payloads),*], index: #index, field_ptr: #field_ptr }),
        stack,
        depth,
    )
}

fn decode_kind(
    variants: &[Variant],
    shadows: &[Option<Shadow>],
    repr: Repr,
    generics: Option<&TokenStream>,
    path: &TokenStream,
) -> (TokenStream, TokenStream, TokenStream) {
    let entries = variants.iter().enumerate().map(|(index, variant)| {
		let index = Literal::u8_unsuffixed(u8::try_from(index).unwrap_or(u8::MAX));
		let assemble = match repr {
			Repr::Rust => quote!(__p::assemble::<Self, #index>),
			Repr::U8 | Repr::CU8 => quote!(__p::write_tag::<#index>),
		};
		let payload = match &variant.payload {
			Payload::Unit => quote!(None),
			Payload::Newtype(ty) => quote!(Some(<#ty as #path>::DECODE)),
			Payload::Struct(fields) => {
				let name = layout_ident(&variant.ident);
				let kinds = fields.iter().map(|Field { ty, .. }| quote!(<#ty as #path>::DECODE));
				quote!(Some(&__p::DecKind::Struct { ops: &__p::DECODE_MAP, layout: #name, kinds: &[#(#kinds),*], drop: None }))
			}
		};
		quote!(__p::DecVariant { payload: #payload, assemble: #assemble })
	});
    let fits = shadows
        .iter()
        .flatten()
        .filter(|_| repr == Repr::Rust)
        .map(|s| {
            let shadow = &s.ident;
            let shadow = quote!(#shadow #generics);
            quote! {
                assert!(
                    ::core::mem::size_of::<#shadow>() <= ::core::mem::size_of::<Self>()
                        && ::core::mem::align_of::<#shadow>() <= ::core::mem::align_of::<Self>(),
                    "cbor-static: a variant payload does not fit its enum; add #[repr(C, u8)]",
                );
            }
        });
    let (stack, depth) = stack(variants, path);
    (
        quote!({
            #(#fits)*
            __p::DecKind::Enum { ops: &__p::DECODE_EXTERNAL, layout: LAYOUT, variants: &[#(#entries),*], drop: __p::drop_fn::<Self>() }
        }),
        stack,
        depth,
    )
}

fn stack(variants: &[Variant], path: &TokenStream) -> (TokenStream, TokenStream) {
    level(variants.iter().map(|variant| match &variant.payload {
        Payload::Unit => (quote!(()), quote!(0)),
        Payload::Newtype(ty) => child(ty, path),
        Payload::Struct(fields) => level(fields.iter().map(|Field { ty, .. }| child(ty, path))),
    }))
}

fn layout_ident(variant: &Ident) -> Ident {
    format_ident!("LAYOUT_{}", variant)
}
