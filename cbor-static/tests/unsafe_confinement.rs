//! R-131 (D-24, D-24a): `unsafe` appears only under `src/engine/`, apart from `unsafe impl` of the
//! codec traits and the two `unsafe trait` declarations. Neither crate, nor a test, shares a
//! mutable static.

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
    let found = found_in(
        sources(&src).into_iter().filter(|path| {
            !path
                .strip_prefix(&src)
                .is_ok_and(|inner| inner.starts_with("engine"))
        }),
        uses,
    );
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
#[cfg_attr(miri, ignore = "parses source text, which no unsafe touches")]
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
#[cfg_attr(miri, ignore = "parses source text, which no unsafe touches")]
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

#[test]
#[cfg_attr(miri, ignore = "reads the source tree")]
fn no_crate_or_test_shares_a_mutable_static() {
    assert_eq!(
        found_in(
            ["src", "tests", "../cbor-static-derive/src"]
                .into_iter()
                .flat_map(|dir| sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join(dir))),
            shared_mutable,
        ),
        Vec::<(PathBuf, String)>::new()
    );
}

#[test]
#[cfg_attr(miri, ignore = "parses source text, which no unsafe touches")]
fn immutable_and_atomic_statics_and_static_lifetimes_are_not_shared_mutable() {
    for allowed in [
        "pub static ENC_STR: EncKind = EncKind::Leaf { size };",
        "static DROPPED: AtomicUsize = AtomicUsize::new(0);",
        "fn f(slot: &'static mut u8) {}",
        "unsafe impl Encode for u32 {}",
        "macro_rules! m { () => { quote!(static TABLE: [u8; 2] = [0, 1];) } }",
    ] {
        assert_eq!(
            shared_mutable(parsed(allowed)),
            Vec::<String>::new(),
            "{allowed}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "parses source text, which no unsafe touches")]
fn static_mut_thread_marker_impls_and_sync_cells_are_shared_mutable() {
    for flagged in [
        "static mut COUNT: usize = 0;",
        "pub(crate) static mut SLOT: Option<u8> = None;",
        "unsafe impl Sync for Slot {}",
        "unsafe impl<T> core::marker::Send for Slot<T> where T: Copy {}",
        "fn f() { static mut LOCAL: u8 = 0; }",
        "macro_rules! m { () => { quote!(static mut TABLE: u8 = 0;) } }",
        "static SLOT: core::cell::SyncUnsafeCell<u8> = core::cell::SyncUnsafeCell::new(0);",
    ] {
        assert!(!shared_mutable(parsed(flagged)).is_empty(), "{flagged}");
    }
}

fn parsed(source: &str) -> TokenStream {
    syn::parse_file(source).expect("parses").into_token_stream()
}

/// What `pick` finds in each file at `paths`.
fn found_in<T>(
    paths: impl Iterator<Item = PathBuf>,
    pick: impl Fn(TokenStream) -> Vec<T>,
) -> Vec<(PathBuf, T)> {
    paths
        .flat_map(|path| {
            pick(parsed(&fs::read_to_string(&path).expect("reads")))
                .into_iter()
                .map(move |found| (path.clone(), found))
        })
        .collect()
}

/// What `pick` finds at each token of `tokens`, macro bodies included. `pick` sees the token's
/// whole stream and its index in it.
fn scan<T>(tokens: TokenStream, pick: &impl Fn(&[TokenTree], usize) -> Option<T>) -> Vec<T> {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    trees
        .iter()
        .enumerate()
        .flat_map(|(at, tree)| match tree {
            TokenTree::Group(group) => scan(group.stream(), pick),
            _ => pick(&trees, at).into_iter().collect(),
        })
        .collect()
}

/// Every `unsafe` keyword in `tokens`, classified.
fn uses(tokens: TokenStream) -> Vec<Unsafe> {
    scan(tokens, &|trees, at| {
        is(&trees[at], "unsafe").then(|| classify(&trees[at + 1..]))
    })
}

/// Every `static mut`, `unsafe impl` of `Send` or `Sync`, and `SyncUnsafeCell` in `tokens`: what
/// code needs to share a non-atomic mutable static.
fn shared_mutable(tokens: TokenStream) -> Vec<String> {
    scan(tokens, &|trees, at| match (&trees[..at], &trees[at..]) {
        ([.., TokenTree::Punct(lifetime)], _) if lifetime.as_char() == '\'' => None,
        (_, [keyword, next, ..]) if is(keyword, "static") && is(next, "mut") => {
            Some(excerpt(&trees[at..]))
        }
        (_, [keyword, next, header @ ..])
            if is(keyword, "unsafe")
                && is(next, "impl")
                && trait_name(header)
                    .is_some_and(|name| ["Send", "Sync"].contains(&name.as_str())) =>
        {
            Some(excerpt(&trees[at..]))
        }
        (_, [cell, ..]) if is(cell, "SyncUnsafeCell") => Some(excerpt(&trees[at..])),
        _ => None,
    })
}

fn excerpt(trees: &[TokenTree]) -> String {
    TokenStream::from_iter(trees.iter().take(6).cloned()).to_string()
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
        _ => Unsafe::Other(excerpt(after)),
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
