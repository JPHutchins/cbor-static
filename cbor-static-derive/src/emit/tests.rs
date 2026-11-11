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
        parse_quote!(
            #[cbor(keys = Label, deny_unknown_fields)]
            struct Keyed {
                alg: i32,
            }
        ),
        parse_quote!(
            struct Mixed {
                #[cbor(key = -1)]
                a: u32,
                b: u32,
            }
        ),
        parse_quote!(
            #[cbor(array)]
            struct Positional {
                a: u32,
                b: Option<u32>,
            }
        ),
        parse_quote!(
            #[cbor(index)]
            enum Indexed {
                A,
                B(u32),
            }
        ),
        parse_quote!(
            enum Tagged {
                #[cbor(cbor_tag = 18)]
                A(u32),
                #[cbor(cbor_tag = 17)]
                B { x: u32 },
            }
        ),
        parse_quote!(
            #[serde(tag = "t")]
            enum Internal {
                A,
                B { x: u32 },
            }
        ),
        parse_quote!(
            #[serde(tag = "t", content = "c")]
            enum Adjacent {
                A,
                B(u32),
            }
        ),
        parse_quote!(
            #[serde(untagged)]
            enum Untagged {
                A,
                B(u32),
            }
        ),
    ] {
        assert_eq!(after_unsafe(&input), ["impl", "impl"]);
    }
}
