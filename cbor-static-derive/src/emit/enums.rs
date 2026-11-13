use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::Ident;

use super::shadows::{Placement, Shadow, mirror, owner, shadow};
use super::structs::decode_map_ops;
use super::{
    Body, Trait, field_child, field_decode, field_encode, field_kind, field_stack, forms, impls,
    key, layout, level,
};
use crate::Directions;
use crate::model::{EnumForm, Field, Item, Offsets, Payload, Repr, Unknown, Variant};

pub fn enumeration(
    item: &Item,
    variants: &[Variant],
    (repr, offsets): (Repr, Offsets),
    form: &EnumForm,
    directions: Directions,
) -> TokenStream {
    let ops = forms::ops(form);
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
    let mirror_definition =
        (repr == Repr::CU8).then(|| mirror(ident, &shadows, generics.as_ref(), statics.as_ref()));
    let placement = Placement {
        ident,
        statics: statics.as_ref(),
        repr,
        offsets,
    };
    let variant_layout = layout(
        &format_ident!("LAYOUT"),
        variants
            .iter()
            .zip(&shadows)
            .zip(forms::variant_keys(form, variants))
            .map(|((variant, shadow), key)| {
                let offset = shadow
                    .as_ref()
                    .filter(|_| {
                        matches!(variant.payload, Payload::Newtype(_)) && repr != Repr::Rust
                    })
                    .map_or_else(
                        || quote!(0),
                        |s| placement.offset(&variant.ident, &s.ident, None),
                    );
                (key, offset, false)
            })
            .chain(
                forms::extras(form)
                    .into_iter()
                    .map(|key| (key, quote!(0), false)),
            ),
    );
    let payload_layouts = variants
        .iter()
        .zip(&shadows)
        .filter_map(|(variant, shadow)| match (&variant.payload, shadow) {
            (Payload::Struct(fields), Some(s)) => Some(layout(
                &layout_ident(&variant.ident),
                fields.iter().map(|field| {
                    (
                        key(&field.key),
                        placement.offset(&variant.ident, &s.ident, Some(&field.member)),
                        field.omit_if_absent,
                    )
                }),
            )),
            _ => None,
        });
    let definitions = shadows.iter().flatten().map(|s| &s.definition);
    let glue = (repr == Repr::Rust).then(|| glue(item, variants, &shadows));
    let encode = impls(item, directions, Trait::Encode, |path| {
        encode_kind(variants, repr, form, &ops.encode, path)
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        decode_kind(
            ident,
            variants,
            &shadows,
            repr,
            generics.as_ref(),
            form,
            path,
        )
    });
    quote! {
        #(#definitions)*
        #mirror_definition
        #variant_layout
        #(#payload_layouts)*
        #glue
        #encode
        #decode
    }
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
    form: &EnumForm,
    ops: &TokenStream,
    path: &TokenStream,
) -> Body {
    let glue = (repr == Repr::Rust).then(|| quote!(Some(__p::field_ptr_of::<Self>)));
    let field_ptr = glue.clone().unwrap_or_else(|| quote!(None));
    let index = glue
        .as_ref()
        .map_or_else(|| quote!(__p::index_tag), |_| quote!(__p::index_of::<Self>));
    let payloads = variants.iter().map(|variant| match &variant.payload {
		Payload::Unit => quote!(None),
		Payload::Newtype(field) => {
			let check = forms::newtype_check(form, &field.ty);
			let kind = field_kind(&field.ty, field.bytes, Trait::Encode, path);
			quote!(Some({ #check #kind }))
		}
		Payload::Struct(fields) => {
			let name = layout_ident(&variant.ident);
			let kinds = fields.iter().map(|field| field_encode(field, path));
			quote!(Some(&__p::EncKind::Struct { ops: &__p::ENCODE_MAP, layout: #name, kinds: &[#(#kinds),*], field_ptr: #field_ptr }))
		}
	});
    let (stack, depth) = stack(variants, path, false);
    Body {
        kind: quote!(&__p::EncKind::Enum { ops: &#ops, layout: LAYOUT, payloads: &[#(#payloads),*], index: #index, field_ptr: #field_ptr }),
        stack,
        depth,
        lead: TokenStream::new(),
    }
}

fn decode_kind(
    ident: &Ident,
    variants: &[Variant],
    shadows: &[Option<Shadow>],
    repr: Repr,
    generics: Option<&TokenStream>,
    form: &EnumForm,
    path: &TokenStream,
) -> Body {
    let ops = forms::ops(form).decode;
    let entries = variants.iter().enumerate().map(|(index, variant)| {
		let index = Literal::u8_unsuffixed(u8::try_from(index).unwrap_or(u8::MAX));
		let assemble = match repr {
			Repr::Rust => quote!(__p::assemble::<Self, #index>),
			Repr::U8 | Repr::CU8 => quote!(__p::write_tag::<#index>),
		};
		let payload = match &variant.payload {
			Payload::Unit => quote!(None),
			Payload::Newtype(field) => {
				let check = forms::newtype_check(form, &field.ty);
				let kind = field_kind(&field.ty, field.bytes, Trait::Decode, path);
				quote!(Some({ #check #kind }))
			}
			Payload::Struct(fields) => {
				let name = layout_ident(&variant.ident);
				let owner = owner(ident, variant);
				let kinds = fields.iter().enumerate().map(|(index, field)| field_decode(field, index, &owner, generics, path));
				let ops = decode_map_ops(fields, Unknown::Skip);
				quote!(Some(&__p::DecKind::Struct { ops: &#ops, layout: #name, kinds: &[#(#kinds),*], drop: None }))
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
    let (stack, depth) = stack(variants, path, unit_payload_frames(form));
    let layout = forms::decode_layout(form, variants, path);
    Body {
        kind: quote!(&{
            #(#fits)*
            __p::DecKind::Enum { ops: &#ops, layout: #layout, variants: &[#(#entries),*], drop: __p::drop_fn::<Self>() }
        }),
        stack,
        depth,
        lead: forms::lead(form, variants, path),
    }
}

/// One level per enum frame, over each variant's payload. A unit variant has none, unless
/// `unit_frames`: an internally tagged unit variant decodes through an empty payload map.
fn stack(
    variants: &[Variant],
    path: &TokenStream,
    unit_frames: bool,
) -> (TokenStream, TokenStream) {
    level(variants.iter().map(|variant| match &variant.payload {
        Payload::Unit if unit_frames => level(core::iter::empty()),
        Payload::Unit => (quote!(()), quote!(0)),
        Payload::Newtype(field) => field_child(&field.ty, field.bytes, path),
        Payload::Struct(fields) => level(fields.iter().map(|field| field_stack(field, path))),
    }))
}

const fn unit_payload_frames(form: &EnumForm) -> bool {
    matches!(form, EnumForm::Internal { .. })
}

fn layout_ident(variant: &Ident) -> Ident {
    format_ident!("LAYOUT_{}", variant)
}
