//! R-131 (D-24, D-24a): `unsafe` appears only under `src/engine/`, apart from `unsafe impl` of the
//! codec traits and the two `unsafe trait` declarations.

use std::fs;
use std::iter::once;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;

const CODEC_TRAITS: [&str; 2] = ["Encode", "Decode"];

#[derive(Debug, PartialEq, Eq)]
enum Unsafe {
    CodecImpl,
    CodecTrait,
    Other(String),
}

#[test]
#[cfg_attr(miri, ignore = "reads the source tree")]
fn unsafe_outside_the_engine_is_only_the_codec_traits() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found: Vec<(PathBuf, Unsafe)> = sources(&src)
        .into_iter()
        .filter(|path| {
            !path
                .strip_prefix(&src)
                .is_ok_and(|inner| inner.starts_with("engine"))
        })
        .flat_map(|path| {
            uses(parsed(&fs::read_to_string(&path).expect("reads")))
                .into_iter()
                .map(move |found| (path.clone(), found))
        })
        .collect();
    assert_eq!(
        found
            .iter()
            .filter(|(_, found)| matches!(found, Unsafe::Other(_)))
            .collect::<Vec<_>>(),
        Vec::<&(PathBuf, Unsafe)>::new()
    );
    assert_eq!(
        found
            .iter()
            .filter(|(_, found)| *found == Unsafe::CodecTrait)
            .count(),
        CODEC_TRAITS.len()
    );
    assert!(found.iter().any(|(_, found)| *found == Unsafe::CodecImpl));
}

#[test]
fn codec_impls_and_declarations_are_allowed_in_items_and_macros() {
    for allowed in [
        "unsafe impl Encode for u32 {}",
        "unsafe impl<'de: 'a, 'a> Decode<'de> for &'a str {}",
        "unsafe impl<T: Encode, const N: usize> crate::schema::Encode for [T; N] {}",
        "unsafe impl<F: Fn() -> u8> Encode for Wrapper<F> where F: Send {}",
        "macro_rules! leaf { ($ty:ty) => { unsafe impl<'de> Decode<'de> for $ty {} } }",
        "macro_rules! tuple { ($($T:ident),+) => { unsafe impl<$($T: Encode),+> Encode for ($($T,)+) {} } }",
    ] {
        assert_eq!(uses(parsed(allowed)), [Unsafe::CodecImpl], "{allowed}");
    }
    assert_eq!(
        uses(parsed(
            "pub unsafe trait Encode: Sized {} pub unsafe trait Decode<'de> {}"
        )),
        [Unsafe::CodecTrait, Unsafe::CodecTrait]
    );
}

#[test]
fn every_other_unsafe_is_reported() {
    for flagged in [
        "unsafe impl Send for Slot {}",
        "unsafe impl<T> Sync for Encode<T> {}",
        "pub unsafe trait Other {}",
        "fn f() { unsafe { g() } }",
        "unsafe fn f() {}",
        "pub type DecodeFn = unsafe fn(*mut u8);",
        "#[unsafe(no_mangle)] fn f() {}",
        "unsafe extern \"C\" {}",
        "macro_rules! m { () => { unsafe { g() } } }",
    ] {
        assert!(
            matches!(uses(parsed(flagged)).as_slice(), [Unsafe::Other(_)]),
            "{flagged}"
        );
    }
}

fn parsed(source: &str) -> TokenStream {
    syn::parse_file(source).expect("parses").into_token_stream()
}

/// Every `unsafe` keyword in `tokens`, macro bodies included, classified.
fn uses(tokens: TokenStream) -> Vec<Unsafe> {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    trees
        .iter()
        .enumerate()
        .flat_map(|(at, tree)| match tree {
            TokenTree::Ident(ident) if ident == "unsafe" => vec![classify(&trees[at + 1..])],
            TokenTree::Group(group) => uses(group.stream()),
            _ => vec![],
        })
        .collect()
}

fn classify(after: &[TokenTree]) -> Unsafe {
    match after {
        [keyword, TokenTree::Ident(name), ..]
            if is(keyword, "trait") && CODEC_TRAITS.iter().any(|codec| name == codec) =>
        {
            Unsafe::CodecTrait
        }
        [keyword, header @ ..]
            if is(keyword, "impl")
                && trait_name(header).is_some_and(|name| CODEC_TRAITS.contains(&name.as_str())) =>
        {
            Unsafe::CodecImpl
        }
        _ => Unsafe::Other(TokenStream::from_iter(after.iter().take(6).cloned()).to_string()),
    }
}

/// The trait's last path segment in `impl<generics> path::Trait<args> for Type {`: the last
/// identifier outside angle brackets before `for`.
fn trait_name(header: &[TokenTree]) -> Option<String> {
    let outside: Vec<&TokenTree> = header
        .split(|tree| matches!(tree, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace))
        .next()
        .into_iter()
        .flat_map(|before_body| before_body.iter().zip(depths(before_body)))
        .filter_map(|(tree, depth)| (depth == 0).then_some(tree))
        .collect();
    outside
        .iter()
        .take_while(|tree| !is(tree, "for"))
        .filter_map(|tree| match tree {
            TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        })
        .last()
        .filter(|_| outside.iter().any(|tree| is(tree, "for")))
}

/// The angle-bracket depth before each token. The `>` of `->` closes nothing.
fn depths(trees: &[TokenTree]) -> impl Iterator<Item = i32> + '_ {
    trees
        .iter()
        .zip(once(None).chain(trees.iter().map(Some)))
        .scan(0, |depth, (tree, previous)| {
            let before = *depth;
            *depth += match (tree, previous) {
                (TokenTree::Punct(p), _) if p.as_char() == '<' => 1,
                (TokenTree::Punct(p), Some(TokenTree::Punct(arrow)))
                    if p.as_char() == '>' && arrow.as_char() == '-' =>
                {
                    0
                }
                (TokenTree::Punct(p), _) if p.as_char() == '>' => -1,
                _ => 0,
            };
            Some(before)
        })
}

fn is(tree: &TokenTree, word: &str) -> bool {
    matches!(tree, TokenTree::Ident(ident) if ident == word)
}

fn sources(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .expect("lists")
        .map(|entry| entry.expect("lists").path())
        .flat_map(|path| {
            if path.is_dir() {
                sources(&path)
            } else {
                path.extension()
                    .is_some_and(|ext| ext == "rs")
                    .then_some(path)
                    .into_iter()
                    .collect()
            }
        })
        .collect()
}
