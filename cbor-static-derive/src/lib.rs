//! Derives for `cbor-static`: each emits constant descriptors and, for enums without a
//! primitive `repr`, safe glue. No derive output contains `unsafe`.

mod emit;
mod model;

use proc_macro::TokenStream;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Directions {
    Encode,
    Decode,
    Both,
}

#[proc_macro_derive(Encode, attributes(cbor, serde))]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    expand(input, Directions::Encode)
}

#[proc_macro_derive(Decode, attributes(cbor, serde))]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    expand(input, Directions::Decode)
}

#[proc_macro_derive(Cbor, attributes(cbor, serde))]
pub fn derive_cbor(input: TokenStream) -> TokenStream {
    expand(input, Directions::Both)
}

fn expand(input: TokenStream, directions: Directions) -> TokenStream {
    syn::parse::<syn::DeriveInput>(input)
        .and_then(|ast| model::Item::parse(&ast))
        .map_or_else(syn::Error::into_compile_error, |item| {
            emit::item(&item, directions)
        })
        .into()
}
