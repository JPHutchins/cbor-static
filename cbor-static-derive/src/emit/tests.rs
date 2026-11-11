use proc_macro2::{TokenStream, TokenTree};
use syn::{DeriveInput, parse_quote};

use super::item;
use crate::Directions;
use crate::model::Item;

fn leaves(tokens: TokenStream) -> Vec<TokenTree> {
    tokens
        .into_iter()
        .flat_map(|tree| match tree {
            TokenTree::Group(group) => leaves(group.stream()),
            leaf => vec![leaf],
        })
        .collect()
}

fn after_unsafe(input: &DeriveInput) -> Vec<String> {
    leaves(item(
        &Item::parse(input).expect("a supported item"),
        Directions::Both,
    ))
    .windows(2)
    .filter(|pair| matches!(&pair[0], TokenTree::Ident(ident) if ident == "unsafe"))
    .map(|pair| pair[1].to_string())
    .collect()
}

#[test]
fn the_only_unsafe_in_derive_output_is_the_codec_trait_impls() {
    for input in [
        parse_quote!(
            struct Flat<'a> {
                a: u32,
                b: &'a str,
            }
        ),
        parse_quote!(
            enum Glue {
                A,
                B(u32),
                C { x: u32 },
            }
        ),
        parse_quote!(
            #[repr(u8)]
            enum Tag {
                A,
                B(u32),
                C { x: u32 },
            }
        ),
        parse_quote!(
            #[repr(C, u8)]
            enum Union {
                A,
                B(u32),
                C { x: u32 },
            }
        ),
    ] {
        assert_eq!(after_unsafe(&input), ["impl", "impl"]);
    }
}
