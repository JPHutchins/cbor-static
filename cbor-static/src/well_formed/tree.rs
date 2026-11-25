//! Generated well-formed CBOR trees, with random head widths, indefinite containers and chunked
//! strings.

use std::boxed::Box;
use std::string::String;
use std::vec;
use std::vec::Vec;

use proptest::collection::vec as many;
use proptest::prelude::*;

#[derive(Clone, Debug)]
pub(crate) enum Tree {
    Int {
        major: u8,
        arg: u64,
        width: u8,
    },
    Bytes {
        payload: Vec<u8>,
        width: u8,
    },
    Text {
        payload: String,
        width: u8,
    },
    Chunked {
        text: bool,
        chunks: Vec<String>,
    },
    Simple(u8),
    Float {
        width: u8,
        bits: u64,
    },
    Array {
        items: Vec<Tree>,
        indefinite: bool,
        width: u8,
    },
    Map {
        entries: Vec<(Tree, Tree)>,
        indefinite: bool,
        width: u8,
    },
    Tag {
        number: u64,
        width: u8,
        content: Box<Tree>,
    },
}

pub(crate) fn tree() -> impl Strategy<Value = Tree> {
    let leaf = prop_oneof![
        (0u8..2, any::<u64>(), 0u8..5).prop_map(|(major, arg, width)| Tree::Int {
            major,
            arg,
            width
        }),
        (many(any::<u8>(), 0..6), 0u8..5)
            .prop_map(|(payload, width)| Tree::Bytes { payload, width }),
        (".{0,4}", 0u8..5).prop_map(|(payload, width)| Tree::Text { payload, width }),
        (any::<bool>(), many(".{0,3}", 0..3))
            .prop_map(|(text, chunks)| Tree::Chunked { text, chunks }),
        prop_oneof![0u8..24, 32u8..=255].prop_map(Tree::Simple),
        (0u8..3, any::<u64>()).prop_map(|(width, bits)| Tree::Float { width, bits }),
    ];
    leaf.prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            10 => (many(inner.clone(), 0..4), any::<bool>(), 0u8..5).prop_map(
                |(items, indefinite, width)| Tree::Array {
                    items,
                    indefinite,
                    width
                }
            ),
            10 => (
                many((inner.clone(), inner.clone()), 0..3),
                any::<bool>(),
                0u8..5
            )
                .prop_map(|(entries, indefinite, width)| Tree::Map {
                    entries,
                    indefinite,
                    width
                }),
            10 => (0u64..300, 0u8..5, inner).prop_map(|(number, width, content)| Tree::Tag {
                number,
                width,
                content: Box::new(content),
            }),
            1 => bignum(),
        ]
    })
}

/// A tag 2 or 3 over byte strings on either side of the lengths where a bignum outgrows a `u64`
/// and where it outgrows what ciborium reads as an integer.
fn bignum() -> impl Strategy<Value = Tree> {
    (2u64..=3, 0u8..5, many(any::<u8>(), 9..=17), 0u8..5).prop_map(
        |(number, width, payload, payload_width)| Tree::Tag {
            number,
            width,
            content: Box::new(Tree::Bytes {
                payload,
                width: payload_width,
            }),
        },
    )
}

/// A head of `major` and `arg`: its argument in the shortest form, widened to 1, 2, 4 or 8 bytes by
/// `width` 1 to 4 where it fits there.
fn head(out: &mut Vec<u8>, major: u8, arg: u64, width: u8) {
    let shortest = match arg {
        0..0x18 => 0,
        0x18..0x100 => 1,
        0x100..0x1_0000 => 2,
        0x1_0000..0x1_0000_0000 => 4,
        _ => 8,
    };
    let len = [0, 1, 2, 4, 8][usize::from(width)].max(shortest);
    let bytes = arg.to_be_bytes();
    out.push(
        major << 5
            | match len {
                0 => bytes[7],
                1 => 0x18,
                2 => 0x19,
                4 => 0x1a,
                _ => 0x1b,
            },
    );
    out.extend(&bytes[8 - len..]);
}

fn length(len: usize) -> u64 {
    u64::try_from(len).expect("a test length")
}

fn encode(tree: &Tree, out: &mut Vec<u8>) {
    match tree {
        Tree::Int { major, arg, width } => head(out, *major, *arg, *width),
        Tree::Bytes { payload, width } => {
            head(out, 2, length(payload.len()), *width);
            out.extend(payload);
        }
        Tree::Text { payload, width } => {
            head(out, 3, length(payload.len()), *width);
            out.extend(payload.as_bytes());
        }
        Tree::Chunked { text, chunks } => {
            let major = if *text { 3 } else { 2 };
            out.push(major << 5 | 0x1f);
            for chunk in chunks {
                head(out, major, length(chunk.len()), 0);
                out.extend(chunk.as_bytes());
            }
            out.push(0xff);
        }
        Tree::Simple(value @ 0..24) => out.push(0xe0 | value),
        Tree::Simple(value) => out.extend([0xf8, *value]),
        Tree::Float { width, bits } => match width {
            0 => out.extend([&[0xf9], &bits.to_be_bytes()[6..]].concat()),
            1 => out.extend([&[0xfa], &bits.to_be_bytes()[4..]].concat()),
            _ => out.extend([&[0xfb], &bits.to_be_bytes()[..]].concat()),
        },
        Tree::Array {
            items,
            indefinite,
            width,
        } => {
            container(out, 4, *indefinite, length(items.len()), *width);
            for item in items {
                encode(item, out);
            }
            close(out, *indefinite);
        }
        Tree::Map {
            entries,
            indefinite,
            width,
        } => {
            container(out, 5, *indefinite, length(entries.len()), *width);
            for (key, value) in entries {
                encode(key, out);
                encode(value, out);
            }
            close(out, *indefinite);
        }
        Tree::Tag {
            number,
            width,
            content,
        } => {
            head(out, 6, *number, *width);
            encode(content, out);
        }
    }
}

fn container(out: &mut Vec<u8>, major: u8, indefinite: bool, len: u64, width: u8) {
    if indefinite {
        out.push(major << 5 | 0x1f);
    } else {
        head(out, major, len, width);
    }
}

fn close(out: &mut Vec<u8>, indefinite: bool) {
    if indefinite {
        out.push(0xff);
    }
}

pub(crate) fn encoded(tree: &Tree) -> Vec<u8> {
    let mut out = vec![];
    encode(tree, &mut out);
    out
}
