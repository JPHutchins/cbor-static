//! What a type's encoding commits to about map key order (D-11b): its `Order`, the ops a root of it
//! selects, and the asserts that keep every map under a deterministic type sorted alike.

use proc_macro2::TokenStream;
use quote::quote;

use crate::model::{Field, Key, Mode};

/// What an `Encode` impl says about key order (D-11b): the `Order` its type commits to, unless it
/// is the trait's `Any`, and the deterministic ops a root of this type selects.
#[derive(Default)]
pub struct Order {
    pub order: Option<TokenStream>,
    pub selects: Option<TokenStream>,
}

/// The `Order` of a type that writes its own maps in `mode`, and the ops a root of it selects.
pub fn ordered(mode: Mode) -> Order {
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
pub fn joined(mode: Mode, parts: impl Iterator<Item = TokenStream>) -> Order {
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
pub fn admitted(mode: Mode, parts: impl Iterator<Item = (String, TokenStream)>) -> TokenStream {
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
pub fn part_order(field: &Field, path: &TokenStream) -> Option<TokenStream> {
    let ty = &field.ty;
    match field.key {
        Key::Skipped => None,
        _ if field.bytes => None,
        Key::Text(_) | Key::Int(_) | Key::Label { .. } | Key::Position | Key::Flattened => {
            Some(quote!(<#ty as #path>::ORDER))
        }
        Key::Other => Some(quote!(<#ty as __p::Other>::ORDER)),
    }
}
