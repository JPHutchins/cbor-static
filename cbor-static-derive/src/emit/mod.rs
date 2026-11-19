mod contiguous;
mod enums;
mod flatten;
mod forms;
mod order;
mod params;
mod shadows;
mod sizes;
mod structs;
mod withs;

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::Expr;

use crate::Directions;
use crate::model::{Fallback, Field, Item, Key, Mode, Shape, StructForm};
use order::{Order, admitted, joined, ordered, part_order};
use params::{Params, codec_generics, generic, params};
use withs::{Site, via, withs};

#[derive(Clone, Copy)]
enum Trait {
    Encode,
    Decode,
}

pub fn item(item: &Item, directions: Directions) -> TokenStream {
    let body = match &item.shape {
        Shape::Struct {
            fields,
            form: StructForm::Map { unknown },
            order,
        } if fields.iter().any(flatten::flattened) => {
            flatten::flattening(item, fields, (*unknown, *order), directions)
        }
        Shape::Struct {
            fields,
            form,
            order,
        } => structs::structure(item, fields, (*form, *order), directions),
        Shape::Unit => structs::transparent(item, None, Mode::Declared, directions),
        Shape::Newtype { field, order } => {
            structs::transparent(item, Some(field.as_ref()), *order, directions)
        }
        Shape::Enum {
            variants,
            repr,
            form,
            offsets,
            order,
        } => enums::enumeration(item, variants, (*repr, *offsets, *order), form, directions),
    };
    let max_encoded_len = sizes::max_encoded_len(item, directions);
    quote! {
        #[allow(non_camel_case_types, non_upper_case_globals, dead_code, clippy::all, clippy::pedantic)]
        const _: () = {
            use ::cbor_static::__private as __p;
            #body
            #max_encoded_len
        };
    }
}

/// A codec trait impl's items: `kind` is the descriptor's `&'static` expression, `lead` is
/// `Decode`'s alone, and `order` is `Encode`'s alone.
struct Body {
    kind: TokenStream,
    stack: TokenStream,
    depth: TokenStream,
    lead: TokenStream,
    order: Order,
}

/// Whether `directions` derives `which`. Each derive expands alone, so an impl of a trait other than
/// the codec traits goes with the one direction that uses it, or `#[derive(Encode, Decode)]` would
/// implement it twice (E0119).
const fn derives(directions: Directions, which: Trait) -> bool {
    matches!(
        (which, directions),
        (Trait::Encode, Directions::Encode | Directions::Both)
            | (Trait::Decode, Directions::Decode | Directions::Both)
    )
}

/// The `Encode` or `Decode` impl, when `directions` asks for it, around the body built from the
/// trait path.
fn impls(
    item: &Item,
    directions: Directions,
    which: Trait,
    body: impl FnOnce(&TokenStream) -> Body,
) -> TokenStream {
    let ident = &item.ident;
    let Params { named, .. } = params(item);
    let ty = quote!(#ident #named);
    let codec = codec_generics(item, which);
    let (generics, _, bounds) = codec.split_for_impl();
    match (which, directions) {
        (Trait::Encode, Directions::Encode | Directions::Both) => {
            let Body {
                kind,
                stack,
                depth,
                order: Order { order, selects },
                ..
            } = body(&quote!(::cbor_static::Encode));
            let order = order.map(|order| quote!(const ORDER: __p::Order = #order;));
            let selects = selects.map(|ops| {
                quote!(const DETERMINISTIC: ::core::option::Option<&'static __p::DeterministicOps> = ::core::option::Option::Some(&#ops);)
            });
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Encode for #ty #bounds {
                    const ENCODE: &'static __p::EncKind = #kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                    #order
                    #selects
                }
            }
        }
        (Trait::Decode, Directions::Decode | Directions::Both) => {
            let Body {
                kind,
                stack,
                depth,
                lead,
                ..
            } = body(&quote!(::cbor_static::Decode<'de>));
            let contiguous = contiguous::contiguous(&item.shape);
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Decode<'de> for #ty #bounds {
                    const DECODE: &'static __p::DecKind = #kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                    type OptionStack = __p::Level<Self::Stack>;
                    const LEAD: __p::Lead = #lead;
                    const CONTIGUOUS: bool = #contiguous;
                }
            }
        }
        _ => TokenStream::new(),
    }
}

/// A layout's consts, and how an impl refers to it: by name, or written out where its offsets
/// depend on the item's parameters (R-127).
struct Placed {
    consts: TokenStream,
    reference: TokenStream,
    fields: TokenStream,
}

/// The layout `name`, with its keys and their pre-encoded names, from (key, byte offset,
/// omit-if-absent) per entry. In a key order other than declaration order, the entries are sorted
/// by their pre-encoded names at compile time, in the order [`sorted`] names, which needs no
/// offset; `inline` writes the layout where it is used, since its offsets name `Self`.
fn layout(
    name: &Ident,
    entries: impl Iterator<Item = (TokenStream, TokenStream, bool)>,
    mode: Mode,
    inline: bool,
) -> Placed {
    let keys = format_ident!("{name}_KEYS", name = name);
    let names = format_ident!("{name}_NAMES", name = name);
    let (entry_keys, (fields, unplaced)): (Vec<_>, (Vec<_>, Vec<_>)) = entries
        .enumerate()
        .map(|(index, (key, offset, omit))| {
            (
                key,
                (
                    quote!(const { __p::field(#keys, #index, #offset, #omit) }),
                    quote!(const { __p::field(#keys, #index, 0, #omit) }),
                ),
            )
        })
        .unzip();
    let blob = quote! {
        const #keys: &[__p::Key] = &[#(#entry_keys),*];
        const #names: [u8; const { __p::names_len(#keys) }] = const { __p::names(#keys) };
    };
    let order = sorted(name);
    let count = fields.len();
    let length_first = mode == Mode::LengthFirst;
    let declared = quote!([#(#fields),*]);
    let (consts, placed) = match mode {
        Mode::Declared => (blob, quote!(&#declared)),
        Mode::Bytewise | Mode::LengthFirst => (
            quote! {
                #blob
                const #order: [u8; #count] = __p::sorted(&#names, &[#(#unplaced),*], #length_first);
            },
            quote!(&__p::permuted(#declared, &#order)),
        ),
    };
    let layout = quote!(&__p::Layout { names: &#names, fields: #placed });
    if inline {
        Placed {
            consts,
            reference: quote!(&const { __p::Layout { names: &#names, fields: #placed } }),
            fields: quote!(const { #placed }),
        }
    } else {
        Placed {
            consts: quote! {
                #consts
                const #name: &__p::Layout = #layout;
            },
            reference: quote!(#name),
            fields: quote!(#name.fields),
        }
    }
}

/// The const holding the order [`layout`] sorted layout `name`'s entries into, by their indices in
/// declaration order.
fn sorted(name: &Ident) -> Ident {
    format_ident!("{name}_SORTED", name = name)
}

/// The kinds of layout `name`'s entries, given in declaration order, in the layout's order.
fn arranged(name: &Ident, mode: Mode, kinds: impl Iterator<Item = TokenStream>) -> TokenStream {
    match mode {
        Mode::Declared => quote!(&[#(#kinds),*]),
        Mode::Bytewise | Mode::LengthFirst => {
            let order = sorted(name);
            quote!(&const { __p::permuted([#(#kinds),*], &#order) })
        }
    }
}

fn text_key(name: &str) -> TokenStream {
    quote!(__p::Key::Text(#name))
}

fn key(key: &Key) -> TokenStream {
    match key {
        Key::Text(text) => text_key(text),
        Key::Int(int) => quote!(__p::Key::Int(#int)),
        Key::Label { label, variant } => quote!(__p::Key::Int(#label::#variant as i64)),
        Key::Position | Key::Flattened => quote!(__p::Key::None),
        Key::Other => quote!(__p::Key::Other),
        Key::Skipped => quote!(__p::Key::Skipped),
    }
}

/// One stack level over `children`: `(Level<Widest<…>>, 1 + deeper(…))`, the children's stacks
/// in a balanced tree of unions, so that a type's nesting grows with the log of its fields, not
/// with their number, which overflows rustc's query depth at 64 fields.
fn level(children: impl Iterator<Item = (TokenStream, TokenStream)>) -> (TokenStream, TokenStream) {
    let (widest, deepest) = widest(&children.collect::<Vec<_>>());
    (quote!(__p::Level<#widest>), quote!(1 + #deepest))
}

/// The union of `children`'s stacks and the deepest of their depths, halved until one remains.
fn widest(children: &[(TokenStream, TokenStream)]) -> (TokenStream, TokenStream) {
    match children {
        [] => (quote!(()), quote!(0)),
        [(stack, depth)] => (quote!(__p::Widest<#stack, ()>), depth.clone()),
        _ => {
            let (left, right) = children.split_at(children.len() / 2);
            let ((left, left_depth), (right, right_depth)) = (widest(left), widest(right));
            (
                quote!(__p::Widest<#left, #right>),
                quote!(__p::deeper(#left_depth, #right_depth)),
            )
        }
    }
}

fn child(ty: &impl ToTokens, path: &TokenStream) -> (TokenStream, TokenStream) {
    (
        quote!(<#ty as #path>::Stack),
        quote!(<#ty as #path>::MAX_DEPTH),
    )
}

/// A field's stack and depth under `which` trait: a `#[cbor(bytes)]` field's byte string opens no
/// frame to encode, and its own to decode; a `with` field's module opens one frame to encode, and
/// none to decode.
fn field_child(
    ty: &impl ToTokens,
    (bytes, with): (bool, bool),
    (which, path): (Trait, &TokenStream),
) -> (TokenStream, TokenStream) {
    match (bytes, with, which) {
        (_, true, Trait::Encode) => (quote!(__p::Level<()>), quote!(1)),
        (true, false, Trait::Encode) | (_, true, Trait::Decode) => (quote!(()), quote!(0)),
        (false, false, _) => child(ty, path),
        (true, false, Trait::Decode) => child(ty, &quote!(::cbor_static::ByteString)),
    }
}

/// A field's descriptor under `which` trait at `path`: its type's own, or with `#[cbor(bytes)]`,
/// its byte string's; under `max_len`, one that bounds its length too; for a `with` field, the
/// one its module is called through, `via`.
fn field_kind(
    ty: &impl ToTokens,
    (bytes, max_len): (bool, Option<&Expr>),
    via: Option<&TokenStream>,
    which: Trait,
    path: &TokenStream,
) -> TokenStream {
    match (which, bytes, max_len) {
        _ if let Some(via) = via => match which {
            Trait::Encode => quote!(__p::WithEncoding::<#via>::ENCODE),
            Trait::Decode => quote!(__p::WithDecoding::<'de, #via>::DECODE),
        },
        (Trait::Encode, false, None) => quote!(<#ty as #path>::ENCODE),
        (Trait::Decode, false, None) => quote!(<#ty as #path>::DECODE),
        (Trait::Encode, true, None) => quote!(<#ty as ::cbor_static::ByteString>::ENCODE_BYTES),
        (Trait::Decode, true, None) => quote!(<#ty as ::cbor_static::ByteString>::DECODE_BYTES),
        (Trait::Encode, bytes, Some(max_len)) => {
            let own = field_kind(ty, (bytes, None), None, which, path);
            quote!(&const { __p::encode_limited::<#ty, { #max_len }>(#own) })
        }
        (Trait::Decode, false, Some(max_len)) => {
            quote!(<__p::Limited<#ty, { #max_len }> as __p::LimitedDecode<'de>>::DECODE)
        }
        (Trait::Decode, true, Some(max_len)) => {
            quote!(<__p::Limited<#ty, { #max_len }> as __p::LimitedBytes>::DECODE_BYTES)
        }
    }
}

/// A named field's stack and depth under `which` trait: a `skip`ped field's take none, a flattened
/// field's are its type's below its own frame, whose place its struct's frame takes, and an `other`
/// field takes a frame only to write its entries back.
fn field_stack(field: &Field, which: Trait, path: &TokenStream) -> (TokenStream, TokenStream) {
    let ty = &field.ty;
    match (&field.key, which) {
        (Key::Skipped, _) | (Key::Other, Trait::Decode) => (quote!(()), quote!(0)),
        (Key::Other, Trait::Encode) => (
            quote!(<#ty as __p::Other>::Stack),
            quote!(<#ty as __p::Other>::MAX_DEPTH),
        ),
        (Key::Flattened, _) => (
            quote!(<<#ty as #path>::Stack as __p::Unlevel>::Children),
            quote!(<#ty as #path>::MAX_DEPTH - 1),
        ),
        (Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position, _) => field_child(
            ty,
            (field.bytes, field.with_module.is_some()),
            (which, path),
        ),
    }
}

/// A named field's descriptor for encoding: [`field_kind`]'s, or for a `skip`ped field one that is
/// always absent.
fn field_encode(field: &Field, site: Site, path: &TokenStream) -> TokenStream {
    match field.key {
        Key::Skipped => quote!(&__p::ENCODE_SKIPPED),
        Key::Other => {
            let ty = &field.ty;
            quote!(<#ty as __p::Other>::ENCODE_OTHER)
        }
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position | Key::Flattened => {
            field_kind(
                &field.ty,
                field.coding(),
                via(field, site, Trait::Encode).as_ref(),
                Trait::Encode,
                path,
            )
        }
    }
}

/// A named field's descriptor for decoding: [`field_kind`]'s behind the default the field is given,
/// or for a `skip`ped field, the default alone. `site` and `index` name the field's `MakeDefault`
/// type for `default = "path"`, as [`makers`] defines it.
fn field_decode(field: &Field, index: usize, site: Site, path: &TokenStream) -> TokenStream {
    let ty = &field.ty;
    let write = match &field.fallback {
        Fallback::Type => None,
        Fallback::Default => Some(quote!(__p::write_default::<#ty>)),
        Fallback::Path(_) => {
            let (maker, named) = (maker(site.owner, index), site.named);
            Some(quote!(__p::write_made::<#maker #named>))
        }
    };
    let via = via(field, site, Trait::Decode);
    match (&field.key, write) {
        (Key::Skipped, Some(write)) => {
            quote!(&const { __p::DecKind::skipped(#write, __p::drop_fn::<#ty>()) })
        }
        (Key::Other, _) => quote!(<#ty as __p::Other>::DECODE_OTHER),
        (_, None) => field_kind(ty, field.coding(), via.as_ref(), Trait::Decode, path),
        (_, Some(write)) => {
            let kind = field_kind(ty, field.coding(), via.as_ref(), Trait::Decode, path);
            quote!(&const { __p::DecKind::defaulted(#kind, #write) })
        }
    }
}

/// For each of `owner`'s fields with `default = "path"`, a type whose `MakeDefault` calls the path.
fn makers(owner: &Ident, fields: &[Field], params: &Params) -> TokenStream {
    let Params {
        declared,
        named,
        bounds,
        marker,
    } = params;
    fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| match &field.fallback {
            Fallback::Path(made) => {
                let (name, ty) = (maker(owner, index), &field.ty);
                Some(quote! {
                    pub struct #name #declared #bounds { #marker }
                    impl #declared __p::MakeDefault for #name #named #bounds {
                        type Value = #ty;
                        fn make() -> #ty { #made() }
                    }
                })
            }
            Fallback::Type | Fallback::Default => None,
        })
        .collect()
}

fn maker(owner: &Ident, index: usize) -> Ident {
    format_ident!("__{owner}Default{index}", owner = owner, index = index)
}

/// The types of their own the derive emits for `item`'s `fields`: [`makers`]' and [`withs`].
fn helpers(item: &Item, fields: &[Field], params: &Params, directions: Directions) -> TokenStream {
    [
        makers(&item.ident, fields, params),
        withs(item, &item.ident, fields, directions),
    ]
    .into_iter()
    .collect()
}

fn static_type(item: &Item) -> TokenStream {
    let ident = &item.ident;
    item.lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |_| quote!(#ident<'static>))
}

#[cfg(test)]
mod tests;
