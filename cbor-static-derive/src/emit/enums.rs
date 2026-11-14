use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::Ident;

use super::shadows::{Placement, Shadow, mirror, mirrored, owner, shadow};
use super::{
    Body, Order, Params, Placed, Trait, admitted, arranged, derives, field_child, field_decode,
    field_encode, field_kind, field_stack, forms, generic, impls, joined, key, layout, level,
    ordered, params, part_order, sorted,
};
use crate::Directions;
use crate::model::{EnumForm, Field, Item, Mode, Offsets, Payload, Repr, Variant};

pub fn enumeration(
    item: &Item,
    variants: &[Variant],
    (repr, offsets, mode): (Repr, Offsets, Mode),
    form: &EnumForm,
    directions: Directions,
) -> TokenStream {
    let ops = forms::ops(form);
    let ident = &item.ident;
    let params = params(item);
    let inline = generic(item);
    let placed_in = if inline {
        params.named.clone()
    } else {
        item.lifetime.as_ref().map_or_default(|_| quote!(<'static>))
    };
    let shadows: Vec<Option<Shadow>> = variants
        .iter()
        .map(|variant| shadow(ident, variant, repr, &params))
        .collect();
    let mirror_check = (repr == Repr::CU8).then(|| mirrored(ident, &placed_in));
    let mirror_definition = (repr == Repr::CU8).then(|| {
        let definition = mirror(ident, &shadows, &params);
        let check = (!inline).then(|| quote!(const _: () = { #mirror_check };));
        quote!(#definition #check)
    });
    let placement = Placement {
        ident,
        statics: &placed_in,
        repr,
        offsets,
    };
    let enum_layout = layout(
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
        Mode::Declared,
        inline,
    );
    let payload_layouts = payload_layouts(variants, &shadows, &placement, (mode, inline));
    let layouts = Layouts {
        own: &enum_layout,
        payloads: &payload_layouts,
        checks: &mirror_check.filter(|_| inline).unwrap_or_default(),
    };
    let definitions = shadows.iter().flatten().map(|s| &s.definition);
    let payload_consts = payload_layouts
        .iter()
        .flatten()
        .map(|placed| &placed.consts);
    let enum_consts = &enum_layout.consts;
    let glue = (repr == Repr::Rust).then(|| glue(item, variants, &shadows, (directions, mode)));
    let encode = impls(item, directions, Trait::Encode, |path| {
        encode_kind(variants, (repr, mode), form, (&ops.encode, &layouts), path)
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        decode_kind(
            (ident, &params.named),
            variants,
            &shadows,
            (repr, mode),
            (form, &layouts),
            path,
        )
    });
    quote! {
        #(#definitions)*
        #mirror_definition
        #enum_consts
        #(#payload_consts)*
        #glue
        #encode
        #decode
    }
}

/// Each struct variant's layout, its fields placed in its shadow, sorted in `mode`.
fn payload_layouts(
    variants: &[Variant],
    shadows: &[Option<Shadow>],
    placement: &Placement,
    (mode, inline): (Mode, bool),
) -> Vec<Option<Placed>> {
    variants
        .iter()
        .zip(shadows)
        .map(|(variant, shadow)| match (&variant.payload, shadow) {
            (Payload::Struct(fields), Some(s)) => Some(layout(
                &layout_ident(&variant.ident),
                fields.iter().map(|field| {
                    (
                        key(&field.key),
                        placement.offset(&variant.ident, &s.ident, Some(&field.member)),
                        field.omit_if_absent,
                    )
                }),
                mode,
                inline,
            )),
            _ => None,
        })
        .collect()
}

/// Where an enum's impls find its layouts: its own, its struct variants', and the checks a layout
/// built in the impl needs there.
struct Layouts<'a> {
    own: &'a Placed,
    payloads: &'a [Option<Placed>],
    checks: &'a TokenStream,
}

/// The glue a Rust-repr enum's encoding (`EnumGlue`) and decoding (`Assemble`) go through, each with
/// the direction that uses it. A struct variant's fields are asked for by their place in its layout,
/// which in a key order other than declaration order is the sorted one.
fn glue(
    item: &Item,
    variants: &[Variant],
    shadows: &[Option<Shadow>],
    (directions, mode): (Directions, Mode),
) -> TokenStream {
    let ident = &item.ident;
    let Params {
        declared,
        named,
        bounds,
        ..
    } = params(item);
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
                    match mode {
                        Mode::Declared => quote!((Self::#name { #member: value, .. }, #index) => ::core::ptr::from_ref(value).cast()),
                        Mode::Bytewise | Mode::LengthFirst => {
                            let order = sorted(&layout_ident(name));
                            quote!((Self::#name { #member: value, .. }, field) if #order.get(usize::from(field)) == Some(&#index) => ::core::ptr::from_ref(value).cast())
                        }
                    }
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
                (Payload::Newtype(_), Some(s)) => (s.ty.clone(), quote!(Self::#name(shadow))),
                (Payload::Struct(fields), Some(s)) => {
                    let moves = fields
                        .iter()
                        .map(|Field { member, .. }| quote!(#member: shadow.#member));
                    (s.ty.clone(), quote!(Self::#name { #(#moves),* }))
                }
                _ => (quote!(()), quote!({ let () = shadow; Self::#name })),
            };
            quote! {
                #[automatically_derived]
                impl #declared __p::Assemble<#index> for #ident #named #bounds {
                    type Shadow = #shadow_ty;
                    fn assemble(shadow: Self::Shadow) -> Self { #build }
                }
            }
        });
    let enum_glue = derives(directions, Trait::Encode).then(|| {
        quote! {
            #[automatically_derived]
            impl #declared __p::EnumGlue for #ident #named #bounds {
                fn variant_index(&self) -> u8 { match self { #(#indices),* } }
                fn field_ptr(&self, field: u8) -> *const u8 {
                    match (self, field) { #(#pointers,)* _ => ::core::ptr::null() }
                }
            }
        }
    });
    let assembles = derives(directions, Trait::Decode)
        .then_some(assembles)
        .into_iter()
        .flatten();
    quote! {
        #enum_glue
        #(#assembles)*
    }
}

fn encode_kind(
    variants: &[Variant],
    (repr, mode): (Repr, Mode),
    form: &EnumForm,
    (ops, layouts): (&TokenStream, &Layouts),
    path: &TokenStream,
) -> Body {
    let glue = (repr == Repr::Rust).then(|| quote!(Some(__p::field_ptr_of::<Self>)));
    let field_ptr = glue.clone().unwrap_or_else(|| quote!(None));
    let index = glue
        .as_ref()
        .map_or_else(|| quote!(__p::index_tag), |_| quote!(__p::index_of::<Self>));
    let payloads = variants.iter().zip(layouts.payloads).map(|(variant, placed)| match (&variant.payload, placed) {
        (Payload::Newtype(field), _) => {
            let check = forms::newtype_check(form, &field.ty, Trait::Encode, path);
            let kind = field_kind(&field.ty, field.bytes, Trait::Encode, path);
            quote!(Some({ #check #kind }))
        }
        (Payload::Struct(fields), Some(Placed { reference, .. })) => {
            let name = layout_ident(&variant.ident);
            let kinds = arranged(&name, mode, fields.iter().map(|field| field_encode(field, path)));
            quote!(Some(&__p::EncKind::Struct { ops: &__p::ENCODE_MAP, layout: #reference, kinds: #kinds, field_ptr: #field_ptr }))
        }
        (Payload::Unit | Payload::Struct(_), _) => quote!(None),
    });
    let (stack, depth) = stack(variants, (Trait::Encode, path), false);
    let parts = variants.iter().flat_map(|variant| match &variant.payload {
        Payload::Unit => Vec::new(),
        Payload::Newtype(field) => part_order(field, path)
            .map(|part| (variant.ident.to_string(), part))
            .into_iter()
            .collect(),
        Payload::Struct(fields) => fields
            .iter()
            .filter_map(|field| {
                Some((
                    format!(
                        "{variant}::{member}",
                        variant = variant.ident,
                        member = field.member.to_token_stream()
                    ),
                    part_order(field, path)?,
                ))
            })
            .collect(),
    });
    let checks: TokenStream = [admitted(mode, parts.clone()), layouts.checks.clone()]
        .into_iter()
        .collect();
    let enum_layout = &layouts.own.reference;
    let kind = quote!(__p::EncKind::Enum { ops: &#ops, layout: #enum_layout, payloads: &[#(#payloads),*], index: #index, field_ptr: #field_ptr });
    Body {
        kind: if checks.is_empty() {
            quote!(&#kind)
        } else {
            quote!(&{ #checks #kind })
        },
        stack,
        depth,
        lead: TokenStream::new(),
        order: if variants
            .iter()
            .any(|variant| matches!(variant.payload, Payload::Struct(_)))
            || matches!(form, EnumForm::Internal { .. } | EnumForm::Adjacent { .. })
        {
            ordered(mode)
        } else {
            joined(mode, parts.map(|(_, part)| part))
        },
    }
}

fn decode_kind(
    (ident, named): (&Ident, &TokenStream),
    variants: &[Variant],
    shadows: &[Option<Shadow>],
    (repr, mode): (Repr, Mode),
    (form, layouts): (&EnumForm, &Layouts),
    path: &TokenStream,
) -> Body {
    let ops = forms::ops(form).decode;
    let entries = variants.iter().zip(layouts.payloads).enumerate().map(|(index, (variant, placed))| {
        let index = Literal::u8_unsuffixed(u8::try_from(index).unwrap_or(u8::MAX));
        let assemble = match repr {
            Repr::Rust => quote!(__p::assemble::<Self, #index>),
            Repr::U8 | Repr::CU8 => quote!(__p::write_tag::<#index>),
        };
        let payload = match (&variant.payload, placed) {
            (Payload::Newtype(field), _) => {
                let check = forms::newtype_check(form, &field.ty, Trait::Decode, path);
                let kind = field_kind(&field.ty, field.bytes, Trait::Decode, path);
                quote!(Some({ #check #kind }))
            }
            (Payload::Struct(fields), Some(Placed { reference, .. })) => {
                let name = layout_ident(&variant.ident);
                let owner = owner(ident, variant);
                let kinds = arranged(&name, mode, fields.iter().enumerate().map(|(index, field)| field_decode(field, index, &owner, named, path)));
                quote!(Some(&__p::DecKind::Struct { ops: __p::map_ops(#reference, false), layout: #reference, kinds: #kinds, drop: None }))
            }
            (Payload::Unit | Payload::Struct(_), _) => quote!(None),
        };
        quote!(__p::DecVariant { payload: #payload, assemble: #assemble })
    });
    let fits = shadows
        .iter()
        .flatten()
        .filter(|_| repr == Repr::Rust)
        .map(|s| {
            let shadow = &s.ty;
            quote! {
                assert!(
                    ::core::mem::size_of::<#shadow>() <= ::core::mem::size_of::<Self>()
                        && ::core::mem::align_of::<#shadow>() <= ::core::mem::align_of::<Self>(),
                    "cbor-static: a variant payload does not fit its enum; add #[repr(C, u8)]",
                );
            }
        });
    let (stack, depth) = stack(variants, (Trait::Decode, path), unit_payload_frames(form));
    let layout = forms::decode_layout(form, variants, path, layouts.own);
    let checks = layouts.checks;
    Body {
        kind: quote!(&{
            #(#fits)*
            #checks
            __p::DecKind::Enum { ops: &#ops, layout: #layout, variants: &[#(#entries),*], drop: __p::drop_fn::<Self>() }
        }),
        stack,
        depth,
        lead: forms::lead(form, variants, path),
        order: Order::default(),
    }
}

/// One level per enum frame, over each variant's payload. A unit variant has none, unless
/// `unit_frames`: an internally tagged unit variant decodes through an empty payload map.
fn stack(
    variants: &[Variant],
    (which, path): (Trait, &TokenStream),
    unit_frames: bool,
) -> (TokenStream, TokenStream) {
    level(variants.iter().map(|variant| match &variant.payload {
        Payload::Unit if unit_frames => level(core::iter::empty()),
        Payload::Unit => (quote!(()), quote!(0)),
        Payload::Newtype(field) => field_child(&field.ty, field.bytes, path),
        Payload::Struct(fields) => {
            level(fields.iter().map(|field| field_stack(field, which, path)))
        }
    }))
}

const fn unit_payload_frames(form: &EnumForm) -> bool {
    matches!(form, EnumForm::Internal { .. })
}

fn layout_ident(variant: &Ident) -> Ident {
    format_ident!("LAYOUT_{variant}", variant = variant)
}
