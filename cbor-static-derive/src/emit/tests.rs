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

fn expanded(input: &DeriveInput) -> TokenStream {
    item(
        &Item::parse(input).expect("a supported item"),
        Directions::Both,
    )
}

fn after_unsafe(input: &DeriveInput) -> Vec<String> {
    leaves(expanded(input))
        .windows(2)
        .filter(|pair| matches!(&pair[0], TokenTree::Ident(ident) if ident == "unsafe"))
        .map(|pair| pair[1].to_string())
        .collect()
}

/// The descriptor const fns that assert (`field`, `names`) or size a blob (`names_len`).
const DESCRIPTOR_FNS: [&str; 3] = ["field", "names", "names_len"];

/// Every `__p::` call to one of [`DESCRIPTOR_FNS`], and whether an enclosing `const { }` block
/// holds it.
fn descriptor_calls(tokens: TokenStream, in_const: bool) -> Vec<(String, bool)> {
    use proc_macro2::Delimiter;

    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    trees
        .iter()
        .enumerate()
        .flat_map(|(at, tree)| {
            match tree {
            TokenTree::Group(group) => descriptor_calls(
                group.stream(),
                in_const
                    || (group.delimiter() == Delimiter::Brace
                        && at.checked_sub(1).and_then(|before| trees.get(before)).is_some_and(
                            |before| matches!(before, TokenTree::Ident(word) if word == "const"),
                        )),
            ),
            TokenTree::Ident(name)
                if DESCRIPTOR_FNS.contains(&name.to_string().as_str())
                    && at.checked_sub(3).and_then(|path| trees.get(path)).is_some_and(
                        |path| matches!(path, TokenTree::Ident(module) if module == "__p"),
                    ) =>
            {
                vec![(name.to_string(), in_const)]
            }
            _ => vec![],
        }
        })
        .collect()
}

fn inputs() -> Vec<DeriveInput> {
    structs().into_iter().chain(enums()).collect()
}

fn structs() -> Vec<DeriveInput> {
    vec![
        parse_quote!(
            struct Flat<'a> {
                a: u32,
                b: &'a str,
            }
        ),
        parse_quote!(
            struct Unit;
        ),
        parse_quote!(
            struct Name<'a>(&'a str);
        ),
        parse_quote!(
            struct Pair(u32, Option<u32>);
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
            #[cbor(deterministic)]
            struct Flattening<'a> {
                a: u32,
                #[serde(flatten)]
                inner: Inner<'a>,
            }
        ),
    ]
}

fn enums() -> Vec<DeriveInput> {
    vec![
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
    ]
}

#[test]
fn the_only_unsafe_in_derive_output_is_the_codec_trait_impls() {
    for input in inputs() {
        assert_eq!(after_unsafe(&input), ["impl", "impl"]);
    }
}

#[test]
fn every_descriptor_const_fn_call_is_in_an_inline_const_block() {
    let calls: Vec<(String, bool)> = inputs()
        .iter()
        .flat_map(|input| descriptor_calls(expanded(input), false))
        .collect();
    assert!(
        DESCRIPTOR_FNS
            .iter()
            .all(|name| calls.iter().any(|(call, _)| call == name)),
        "{calls:?}"
    );
    assert!(calls.iter().all(|&(_, in_const)| in_const), "{calls:?}");
}

/// The identifiers that only a field with `default`, `default = "path"` or `skip` brings into
/// derive output (#13 Q5).
const FALLBACK_IDENTS: [&str; 6] = [
    "defaulted",
    "skipped",
    "ENCODE_SKIPPED",
    "write_default",
    "write_made",
    "MakeDefault",
];

fn fallback_idents(input: &DeriveInput) -> Vec<String> {
    leaves(expanded(input))
        .into_iter()
        .filter_map(|tree| match tree {
            TokenTree::Ident(ident) if FALLBACK_IDENTS.contains(&ident.to_string().as_str()) => {
                Some(ident.to_string())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_type_without_default_or_skip_emits_none_of_their_machinery() {
    for input in inputs() {
        assert_eq!(
            fallback_idents(&input),
            Vec::<String>::new(),
            "{ident}",
            ident = input.ident
        );
    }
    let fallbacks: [DeriveInput; 2] = [
        parse_quote!(
            struct Config {
                #[serde(default)]
                a: u32,
                #[serde(default = "five")]
                b: u32,
                #[serde(skip)]
                c: u32,
            }
        ),
        parse_quote!(
            enum Command {
                Move {
                    #[serde(default)]
                    a: u32,
                    #[serde(skip, default = "five")]
                    b: u32,
                },
            }
        ),
    ];
    for input in fallbacks {
        let found = fallback_idents(&input);
        assert!(
            FALLBACK_IDENTS
                .iter()
                .all(|ident| found.iter().any(|name| name == ident)),
            "{ident}: {found:?}",
            ident = input.ident
        );
    }
}
