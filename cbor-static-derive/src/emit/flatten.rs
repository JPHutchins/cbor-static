//! Map-form structs with `flatten` fields, whose children's entries join the struct's own map
//! (R-127, Q34); their layout is built inside each impl. Each child's names, entries and kinds
//! are copied at compile time, at the field's place in declaration order, from the child's own
//! descriptors into arrays whose shapes `Flat` names, so the engine sees one map of at most 64
//! entries.

use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, quote};

use super::{
    Body, Order, Params, Trait, admitted, derives, field_decode, field_encode, field_stack, impls,
    key, level, makers, ordered, params, part_order,
};
use crate::Directions;
use crate::model::{Field, Item, Key, Mode, Unknown};

pub fn flattening(
    item: &Item,
    fields: &[Field],
    (unknown, mode): (Unknown, Mode),
    directions: Directions,
) -> TokenStream {
    let own_keys = fields
        .iter()
        .filter(|field| !flattened(field))
        .map(|field| key(&field.key));
    let params = params(item);
    let ident = &item.ident;
    let flats = flats(item, fields, directions);
    let encode = impls(item, directions, Trait::Encode, |path| {
        let (stack, depth) = level(
            fields
                .iter()
                .map(|field| field_stack(field, Trait::Encode, path)),
        );
        let kinds = fields.iter().map(|field| {
            let ty = &field.ty;
            if flattened(field) {
                quote!(__p::EncKind::kinds(<#ty as #path>::ENCODE))
            } else {
                let kind = field_encode(field, path);
                quote!(&[#kind])
            }
        });
        let parts = fields
            .iter()
            .filter_map(|field| Some((name(field), part_order(field, path)?)));
        let Built {
            checks,
            layout,
            kinds,
        } = built(
            fields,
            (&quote!(__p::Encoding), Trait::Encode, path),
            (quote!(&'static __p::EncKind), kinds.collect()),
            mode,
        );
        let checks = [checks, admitted(mode, parts)];
        Body {
            kind: quote!(&{
                let layout: &'static __p::Layout = #layout;
                #(#checks)*
                __p::EncKind::Struct { ops: __p::encode_ops(layout), layout, kinds: #kinds, field_ptr: None }
            }),
            stack,
            depth,
            lead: TokenStream::new(),
            order: ordered(mode),
        }
    });
    let decode = impls(item, directions, Trait::Decode, |path| {
        let (stack, depth) = level(
            fields
                .iter()
                .map(|field| field_stack(field, Trait::Decode, path)),
        );
        let kinds = fields.iter().enumerate().map(|(index, field)| {
            let ty = &field.ty;
            if flattened(field) {
                quote!(__p::DecKind::kinds(<#ty as #path>::DECODE))
            } else {
                let kind = field_decode(field, index, ident, &params.named, path);
                quote!(&[#kind])
            }
        });
        let Built {
            checks,
            layout,
            kinds,
        } = built(
            fields,
            (&quote!(__p::Decoding), Trait::Decode, path),
            (quote!(&'static __p::DecKind), kinds.collect()),
            mode,
        );
        let deny = matches!(unknown, Unknown::Deny);
        Body {
            kind: quote!(&{
                let layout: &'static __p::Layout = #layout;
                #checks
                __p::DecKind::Struct { ops: __p::map_ops(layout, #deny), layout, kinds: #kinds, drop: __p::drop_fn::<Self>() }
            }),
            stack,
            depth,
            lead: quote!(__p::Lead::MAP),
            order: Order::default(),
        }
    });
    let makers = makers(ident, fields, &params);
    quote! {
        const LAYOUT_KEYS: &[__p::Key] = &[#(#own_keys),*];
        const LAYOUT_NAMES: [u8; const { __p::names_len(LAYOUT_KEYS) }] = const { __p::names(LAYOUT_KEYS) };
        #flats
        #makers
        #encode
        #decode
    }
}

/// The struct's `Flat` and `MapForm` impls in each direction it derives: the shapes of its entries
/// and names, its flattened fields' in their places.
fn flats(item: &Item, fields: &[Field], directions: Directions) -> TokenStream {
    let Params {
        declared,
        named,
        bounds,
        ..
    } = params(item);
    let ident = &item.ident;
    [
        (Trait::Encode, quote!(__p::Encoding)),
        (Trait::Decode, quote!(__p::Decoding)),
    ]
    .into_iter()
    .filter(|(which, _)| derives(directions, *which))
    .map(|(_, direction)| {
        let entries = storage(fields, &direction, &quote!(__CborX));
        let names = fields.iter().filter(|field| flattened(field)).fold(
            quote!([u8; LAYOUT_NAMES.len()]),
            |names, field| {
                let ty = &field.ty;
                quote!(__p::Concat<#names, <#ty as __p::Flat<#direction>>::Names>)
            },
        );
        quote! {
            impl #declared __p::Flat<#direction> for #ident #named #bounds {
                type Entries<__CborX: Copy + 'static> = #entries;
                type Names = #names;
            }
            impl #declared __p::MapForm<#direction> for #ident #named #bounds {}
        }
    })
    .collect()
}

/// The checks on a struct's built layout, the layout, and its kinds in its order.
struct Built {
    checks: TokenStream,
    layout: TokenStream,
    kinds: TokenStream,
}

/// The layout and kinds of `fields` in one direction, each run of kinds given in declaration order:
/// sorted in a key order other than declaration order (D-11b).
fn built(
    fields: &[Field],
    (direction, which, path): (&TokenStream, Trait, &TokenStream),
    (kind, kinds): (TokenStream, Vec<TokenStream>),
    mode: Mode,
) -> Built {
    let flat = quote!(<Self as __p::Flat<#direction>>);
    let children = fields
        .iter()
        .filter(|field| flattened(field))
        .map(|field| child_layout(field, which, path));
    let names = quote!(__p::gathered::<u8, #flat::Names>(&[&LAYOUT_NAMES #(, #children.names)*]));
    let runs = fields.iter().scan(0usize, |own, field| {
        let member = &field.member;
        Some(if flattened(field) {
            let layout = child_layout(field, which, path);
            quote!(__p::Run::Flattened(#layout, ::core::mem::offset_of!(Self, #member)))
        } else {
            let index = Literal::usize_unsuffixed(*own);
            *own += 1;
            let omit = field.omit_if_absent;
            quote!(__p::Run::Own(__p::field(LAYOUT_KEYS, #index, ::core::mem::offset_of!(Self, #member), #omit)))
        })
    });
    let entries =
        quote!(__p::entries::<#flat::Entries<__p::FieldLayout>>(LAYOUT_NAMES.len(), &[#(#runs),*]));
    let kinds = quote!(__p::gathered::<#kind, #flat::Entries<#kind>>(&[#(#kinds),*]));
    let (entries, kinds) = match mode {
        Mode::Declared => (entries, kinds),
        Mode::Bytewise | Mode::LengthFirst => {
            let length_first = mode == Mode::LengthFirst;
            let order = quote!(__p::sorted::<#flat::Entries<u8>>(__p::as_slice(&#names), __p::as_slice(&#entries), #length_first));
            (
                quote!(__p::permuted(#entries, __p::as_slice(&#order))),
                quote!(__p::permuted(#kinds, __p::as_slice(&#order))),
            )
        }
    };
    Built {
        checks: quote! {
            assert!(layout.fields.len() <= 64, "cbor-static: a struct has at most 64 entries, those of its flattened fields included");
            assert!(__p::distinct(layout), "cbor-static: a flattened field's key is also another entry's");
        },
        layout: quote!(&const { __p::Layout { names: __p::as_slice(&const { #names }), fields: __p::as_slice(&const { #entries }) } }),
        kinds: quote!(__p::as_slice(&const { #kinds })),
    }
}

/// The type of one value per entry of `fields`, in declaration order, each flattened field's
/// entries in its place.
fn storage(fields: &[Field], direction: &TokenStream, element: &TokenStream) -> TokenStream {
    fields
        .iter()
        .rev()
        .fold(quote!([#element; 0]), |rest, field| {
            let ty = &field.ty;
            if flattened(field) {
                quote!(__p::Concat<<#ty as __p::Flat<#direction>>::Entries<#element>, #rest>)
            } else {
                quote!(__p::Concat<[#element; 1], #rest>)
            }
        })
}

/// A flattened field's layout, from its type's descriptor in the direction `which`.
fn child_layout(field: &Field, which: Trait, path: &TokenStream) -> TokenStream {
    let ty = &field.ty;
    match which {
        Trait::Encode => quote!(__p::flattened(__p::EncKind::layout(<#ty as #path>::ENCODE))),
        Trait::Decode => quote!(__p::flattened(__p::DecKind::layout(<#ty as #path>::DECODE))),
    }
}

/// Whether a field brings its type's entries into its struct's map.
pub fn flattened(field: &Field) -> bool {
    matches!(field.key, Key::Flattened)
}

fn name(field: &Field) -> String {
    field.member.to_token_stream().to_string()
}
