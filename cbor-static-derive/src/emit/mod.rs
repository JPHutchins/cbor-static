mod enums;
mod flatten;
mod forms;
mod shadows;
mod structs;

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::punctuated::Punctuated;
use syn::token::{Comma, Where};
use syn::{GenericParam, Generics, WhereClause, WherePredicate, parse_quote};

use crate::Directions;
use crate::model::{Fallback, Field, Item, Key, Mode, Shape, StructForm};

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
    quote! {
        #[allow(non_camel_case_types, non_upper_case_globals, dead_code, clippy::all, clippy::pedantic)]
        const _: () = {
            use ::cbor_static::__private as __p;
            #body
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

/// What an `Encode` impl says about key order (D-11b): the `Order` its type commits to, unless it
/// is the trait's `Any`, and the deterministic ops a root of this type selects.
#[derive(Default)]
struct Order {
    order: Option<TokenStream>,
    selects: Option<TokenStream>,
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
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Decode<'de> for #ty #bounds {
                    const DECODE: &'static __p::DecKind = #kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                    type OptionStack = __p::Level<Self::Stack>;
                    const LEAD: __p::Lead = #lead;
                }
            }
        }
        _ => TokenStream::new(),
    }
}

/// How the item's parameters are written (R-127): as an impl or a helper type declares them, as
/// the type names them, its where clause, and a marker field that uses every one of them.
struct Params {
    declared: TokenStream,
    named: TokenStream,
    bounds: TokenStream,
    marker: Option<TokenStream>,
}

fn params(item: &Item) -> Params {
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
fn codec_generics(item: &Item, which: Trait) -> Generics {
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
fn generic(item: &Item) -> bool {
    item.generics.type_params().next().is_some() || item.generics.const_params().next().is_some()
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
    let keys = format_ident!("{}_KEYS", name);
    let names = format_ident!("{}_NAMES", name);
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
            reference: layout,
            fields: placed,
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
    format_ident!("{}_SORTED", name)
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

/// The `Order` of a type that writes its own maps in `mode`, and the ops a root of it selects.
fn ordered(mode: Mode) -> Order {
    Order {
        order: Some(order_of(mode)),
        selects: match mode {
            Mode::Declared => None,
            Mode::Bytewise => Some(quote!(__p::BYTEWISE)),
            Mode::LengthFirst => Some(quote!(__p::LENGTH_FIRST)),
        },
    }
}

fn order_of(mode: Mode) -> TokenStream {
    let order = match mode {
        Mode::Declared => quote!(Declared),
        Mode::Bytewise => quote!(Bytewise),
        Mode::LengthFirst => quote!(LengthFirst),
    };
    quote!(__p::Order::#order)
}

/// The `Order` of a type that writes no map of its own: its parts' (D-11b), or in a key order other
/// than declaration order, that one.
fn joined(mode: Mode, parts: impl Iterator<Item = TokenStream>) -> Order {
    match mode {
        Mode::Declared => Order {
            order: Some(parts.fold(
                quote!(__p::Order::Any),
                |all, part| quote!(#all.join(#part)),
            )),
            selects: None,
        },
        Mode::Bytewise | Mode::LengthFirst => ordered(mode),
    }
}

/// In a key order other than declaration order, an assert that each named part's `Order` is that
/// one or `Any`, so every map under the type is sorted alike (D-11b).
fn admitted(mode: Mode, parts: impl Iterator<Item = (String, TokenStream)>) -> TokenStream {
    if mode == Mode::Declared {
        return TokenStream::new();
    }
    let order = order_of(mode);
    parts
        .map(|(name, part)| {
            let message = format!("cbor-static: `{name}` does not sort its map keys the same way");
            quote!(assert!(#order.admits(#part), #message);)
        })
        .collect()
}

/// A field's `Order` as a part of its container: a `skip`ped field is never written, and a
/// `#[cbor(bytes)]` field is a byte string.
fn part_order(field: &Field, path: &TokenStream) -> Option<TokenStream> {
    let ty = &field.ty;
    match field.key {
        Key::Skipped => None,
        _ if field.bytes => None,
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position | Key::Flattened => {
            Some(quote!(<#ty as #path>::ORDER))
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

/// A field's stack and depth: a `#[cbor(bytes)]` field's byte string opens no frame.
fn field_child(ty: &impl ToTokens, bytes: bool, path: &TokenStream) -> (TokenStream, TokenStream) {
    if bytes {
        (quote!(()), quote!(0))
    } else {
        child(ty, path)
    }
}

/// A field's descriptor under `which` trait at `path`: its type's own, or with `#[cbor(bytes)]`,
/// its byte string's.
fn field_kind(ty: &impl ToTokens, bytes: bool, which: Trait, path: &TokenStream) -> TokenStream {
    match (which, bytes) {
        (Trait::Encode, false) => quote!(<#ty as #path>::ENCODE),
        (Trait::Decode, false) => quote!(<#ty as #path>::DECODE),
        (Trait::Encode, true) => quote!(<#ty as ::cbor_static::ByteString>::ENCODE_BYTES),
        (Trait::Decode, true) => quote!(<#ty as ::cbor_static::ByteString>::DECODE_BYTES),
    }
}

/// A named field's stack and depth: a `skip`ped field's take none, and a flattened field's are its
/// type's below its own frame, whose place its struct's frame takes.
fn field_stack(field: &Field, path: &TokenStream) -> (TokenStream, TokenStream) {
    let ty = &field.ty;
    match field.key {
        Key::Skipped => (quote!(()), quote!(0)),
        Key::Flattened => (
            quote!(<<#ty as #path>::Stack as __p::Unlevel>::Children),
            quote!(<#ty as #path>::MAX_DEPTH - 1),
        ),
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position => {
            field_child(ty, field.bytes, path)
        }
    }
}

/// A named field's descriptor for encoding: [`field_kind`]'s, or for a `skip`ped field one that is
/// always absent.
fn field_encode(field: &Field, path: &TokenStream) -> TokenStream {
    match field.key {
        Key::Skipped => quote!(&__p::ENCODE_SKIPPED),
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position | Key::Flattened => {
            field_kind(&field.ty, field.bytes, Trait::Encode, path)
        }
    }
}

/// A named field's descriptor for decoding: [`field_kind`]'s behind the default the field is given,
/// or for a `skip`ped field, the default alone. `owner` names the field's `MakeDefault` type for
/// `default = "path"`, as [`makers`] defines it.
fn field_decode(
    field: &Field,
    index: usize,
    owner: &Ident,
    named: &TokenStream,
    path: &TokenStream,
) -> TokenStream {
    let ty = &field.ty;
    let write = match &field.fallback {
        Fallback::Type => None,
        Fallback::Default => Some(quote!(__p::write_default::<#ty>)),
        Fallback::Path(_) => {
            let maker = maker(owner, index);
            Some(quote!(__p::write_made::<#maker #named>))
        }
    };
    match (&field.key, write) {
        (Key::Skipped, Some(write)) => {
            quote!(&const { __p::DecKind::skipped(#write, __p::drop_fn::<#ty>()) })
        }
        (_, None) => field_kind(ty, field.bytes, Trait::Decode, path),
        (_, Some(write)) => {
            let kind = field_kind(ty, field.bytes, Trait::Decode, path);
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

fn static_type(item: &Item) -> TokenStream {
    let ident = &item.ident;
    item.lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |_| quote!(#ident<'static>))
}

#[cfg(test)]
mod tests;
