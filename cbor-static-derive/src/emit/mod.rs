mod enums;
mod forms;
mod shadows;
mod structs;

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};

use crate::Directions;
use crate::model::{Fallback, Field, Item, Key, Mode, Shape};

#[derive(Clone, Copy)]
enum Trait {
    Encode,
    Decode,
}

pub fn item(item: &Item, directions: Directions) -> TokenStream {
    let body = match &item.shape {
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
    let ty = item
        .lifetime
        .as_ref()
        .map_or_else(|| quote!(#ident), |lifetime| quote!(#ident<#lifetime>));
    match (which, directions) {
        (Trait::Encode, Directions::Encode | Directions::Both) => {
            let generics = item.lifetime.as_ref().map(|lifetime| quote!(<#lifetime>));
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
                unsafe impl #generics ::cbor_static::Encode for #ty {
                    const ENCODE: &'static __p::EncKind = #kind;
                    type Stack = #stack;
                    const MAX_DEPTH: usize = #depth;
                    #order
                    #selects
                }
            }
        }
        (Trait::Decode, Directions::Decode | Directions::Both) => {
            let generics = item.lifetime.as_ref().map_or_else(
                || quote!(<'de>),
                |lifetime| quote!(<'de: #lifetime, #lifetime>),
            );
            let Body {
                kind,
                stack,
                depth,
                lead,
                ..
            } = body(&quote!(::cbor_static::Decode<'de>));
            quote! {
                #[automatically_derived]
                unsafe impl #generics ::cbor_static::Decode<'de> for #ty {
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

/// The `name: &Layout` const, with its keys and their pre-encoded names, from (key, byte offset,
/// omit-if-absent) per entry. In a key order other than declaration order, the entries are sorted
/// by their pre-encoded names at compile time, in the order [`sorted`] names.
fn layout(
    name: &Ident,
    entries: impl Iterator<Item = (TokenStream, TokenStream, bool)>,
    mode: Mode,
) -> TokenStream {
    let keys = format_ident!("{}_KEYS", name);
    let names = format_ident!("{}_NAMES", name);
    let (entry_keys, fields): (Vec<_>, Vec<_>) = entries
        .enumerate()
        .map(|(index, (key, offset, omit))| {
            (
                key,
                quote!(const { __p::field(#keys, #index, #offset, #omit) }),
            )
        })
        .unzip();
    let length_first = match mode {
        Mode::Declared => {
            return quote! {
                const #keys: &[__p::Key] = &[#(#entry_keys),*];
                const #names: [u8; const { __p::names_len(#keys) }] = const { __p::names(#keys) };
                const #name: &__p::Layout = &__p::Layout { names: &#names, fields: &[#(#fields),*] };
            };
        }
        Mode::Bytewise => false,
        Mode::LengthFirst => true,
    };
    let declared = format_ident!("{}_DECLARED", name);
    let order = sorted(name);
    let count = fields.len();
    quote! {
        const #keys: &[__p::Key] = &[#(#entry_keys),*];
        const #names: [u8; const { __p::names_len(#keys) }] = const { __p::names(#keys) };
        const #declared: [__p::FieldLayout; #count] = [#(#fields),*];
        const #order: [u8; #count] = __p::sorted(&#names, &#declared, #length_first);
        const #name: &__p::Layout = &__p::Layout { names: &#names, fields: &__p::permuted(#declared, &#order) };
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
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position => {
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
        Key::Position => quote!(__p::Key::None),
        Key::Skipped => quote!(__p::Key::Skipped),
    }
}

/// One stack level over `children`: `(Level<Widest<…>>, 1 + deeper(…))`.
fn level(
    children: impl DoubleEndedIterator<Item = (TokenStream, TokenStream)>,
) -> (TokenStream, TokenStream) {
    let (widest, deepest) = children.rev().fold(
        (quote!(()), quote!(0)),
        |(widest, deepest), (stack, depth)| {
            (
                quote!(__p::Widest<#stack, #widest>),
                quote!(__p::deeper(#depth, #deepest)),
            )
        },
    );
    (quote!(__p::Level<#widest>), quote!(1 + #deepest))
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

/// A named field's stack and depth: a `skip`ped field's take none.
fn field_stack(field: &Field, path: &TokenStream) -> (TokenStream, TokenStream) {
    match field.key {
        Key::Skipped => (quote!(()), quote!(0)),
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position => {
            field_child(&field.ty, field.bytes, path)
        }
    }
}

/// A named field's descriptor for encoding: [`field_kind`]'s, or for a `skip`ped field one that is
/// always absent.
fn field_encode(field: &Field, path: &TokenStream) -> TokenStream {
    match field.key {
        Key::Skipped => quote!(&__p::ENCODE_SKIPPED),
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position => {
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
    generics: Option<&TokenStream>,
    path: &TokenStream,
) -> TokenStream {
    let ty = &field.ty;
    let write = match &field.fallback {
        Fallback::Type => None,
        Fallback::Default => Some(quote!(__p::write_default::<#ty>)),
        Fallback::Path(_) => {
            let maker = maker(owner, index);
            Some(quote!(__p::write_made::<#maker #generics>))
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
fn makers(
    owner: &Ident,
    fields: &[Field],
    generics: Option<&TokenStream>,
    marker: Option<&TokenStream>,
) -> TokenStream {
    fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| match &field.fallback {
            Fallback::Path(made) => {
                let (name, ty) = (maker(owner, index), &field.ty);
                Some(quote! {
                    pub struct #name #generics { #marker }
                    impl #generics __p::MakeDefault for #name #generics {
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
