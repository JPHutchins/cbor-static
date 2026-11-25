//! ciborium and minicbor against `Tokens`, through the differential oracles' own legs, on any one
//! item `Tokens` reads, and again on the bytes `TokenWriter` writes from its tokens.

use cbor_static::{Token, Tokens};

use crate::oracles::{Data, SHORT_BIGNUM, legs, readers};

/// How deep ciborium's `Value` nests, by [`depth`]: an item nested deeper is refused as
/// `RecursionLimitExceeded`.
pub const CIBORIUM_DEPTH: usize = 256;

/// The stack each level of nesting may take in the oracles' readers, which build and compare their
/// trees recursively; an item nested beyond [`CIBORIUM_DEPTH`] is checked on a thread with this
/// much stack per level, as the main thread's would overflow.
const STACK_PER_LEVEL: usize = 16 * 1024;

/// ciborium and minicbor each read the one item in `input` as `Tokens` does, after their legs'
/// normalization, and so do they the item `TokenWriter` writes back from its tokens where it has
/// no indefinite length, which `Tokens` reads alike. ciborium refuses an item nested deeper than [`CIBORIUM_DEPTH`].
///
/// # Panics
///
/// When an oracle disagrees, or the item written back reads otherwise.
pub fn agree(input: &[u8]) {
    if let Some(depth) = depth(input) {
        if depth > CIBORIUM_DEPTH {
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size((depth + 1) * STACK_PER_LEVEL)
                    .spawn_scoped(scope, || agree_at(input, depth))
                    .expect("a thread for a deep item")
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            });
        } else {
            agree_at(input, depth);
        }
    }
}

fn agree_at(input: &[u8], depth: usize) {
    let ours = readers::tokens(input).expect("Tokens reads the item it walked");
    let rewritten = (!ours.has_indefinite()).then(|| rewritten(input));
    if let Some(rewritten) = &rewritten {
        assert_eq!(
            readers::tokens(rewritten).as_ref(),
            Ok(&ours),
            "Tokens reads {rewritten:02x?}, which TokenWriter writes from {input:02x?}, otherwise"
        );
    }
    for bytes in core::iter::once(input).chain(rewritten.as_deref()) {
        legs::minicbor(bytes, &ours);
        if depth > CIBORIUM_DEPTH {
            too_deep_for_ciborium(bytes, &ours);
        } else {
            legs::ciborium(bytes, &ours);
        }
    }
}

/// ciborium refuses an item nested deeper than [`CIBORIUM_DEPTH`]: for its depth, unless a value
/// its leg says it refuses comes first.
fn too_deep_for_ciborium(bytes: &[u8], ours: &Data) {
    use ciborium::de::Error;

    match (
        ciborium::from_reader::<ciborium::Value, _>(bytes),
        legs::for_ciborium(ours.clone()),
    ) {
        (Err(Error::RecursionLimitExceeded), _) | (Err(Error::Semantic(..)), Err(_)) => {}
        (theirs, _) => {
            panic!("ciborium reads {bytes:02x?}, nested beyond {CIBORIUM_DEPTH}, as {theirs:?}")
        }
    }
}

/// The bytes `TokenWriter` writes from the tokens of `input`, which holds no indefinite length:
/// `TokenWriter` writes none.
fn rewritten(input: &[u8]) -> Vec<u8> {
    use cbor_static::TokenWriter;

    crate::grown(|out| {
        Tokens::new(input)
            .try_fold(TokenWriter::new(out), |mut writer, token| {
                writer.write(token?)?;
                Ok(writer)
            })
            .map(|writer| writer.written())
    })
    .expect("TokenWriter writes the tokens Tokens reads")
}

/// How deep the one item in `input` nests as ciborium counts: each array, map and tag is a level,
/// an empty one too, but a tag 2 or 3 over a short byte string, which ciborium reads as an integer,
/// and an indefinite-length string are none; `None` when `Tokens` refuses `input` or it holds other than
/// one item.
#[must_use]
pub fn depth(input: &[u8]) -> Option<usize> {
    Tokens::new(input)
        .try_fold(Walk::default(), |walk, token| {
            token.ok().and_then(|token| walk.read(token))
        })
        .filter(|walk| walk.items == 1 && walk.open.is_empty())
        .map(|walk| walk.deepest)
}

/// What a walk over tokens still has open, innermost last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Items(u128),
    UntilBreak,
    Chunks,
    Tag(u64),
}

#[derive(Debug, Default)]
struct Walk {
    open: Vec<Open>,
    items: usize,
    deepest: usize,
}

impl Walk {
    /// The walk after `token`; `None` once it is past the first item.
    fn read(mut self, token: Token<'_>) -> Option<Self> {
        if self.items > 0 {
            return None;
        }
        self.deepest = self.deepest.max(self.level(&token));
        match token {
            Token::Array(Some(0)) | Token::Map(Some(0)) => self.opened(Open::Items(0)).completed(),
            Token::Array(Some(len)) => Some(self.opened(Open::Items(len.into()))),
            Token::Map(Some(len)) => Some(self.opened(Open::Items(2 * u128::from(len)))),
            Token::Array(None) | Token::Map(None) => Some(self.opened(Open::UntilBreak)),
            Token::BytesIndefinite | Token::TextIndefinite => Some(self.opened(Open::Chunks)),
            Token::Tag(tag) => Some(self.opened(Open::Tag(tag))),
            Token::Break => {
                self.open.pop();
                self.completed()
            }
            Token::Bytes(_) | Token::Text(_) if self.open.last() == Some(&Open::Chunks) => {
                Some(self)
            }
            Token::Unsigned(_)
            | Token::Negative(_)
            | Token::Bytes(_)
            | Token::Text(_)
            | Token::Simple(_)
            | Token::Bool(_)
            | Token::Null
            | Token::Undefined
            | Token::Float(_) => self.completed(),
        }
    }

    /// The level `token` reaches: the open arrays, maps and tags around it, and itself if it opens
    /// an array or map, but not the tag 2 or 3 around a short byte string.
    fn level(&self, token: &Token<'_>) -> usize {
        let around = self
            .open
            .iter()
            .filter(|open| **open != Open::Chunks)
            .count();
        match (token, self.open.last()) {
            (Token::Array(_) | Token::Map(_), _) => around + 1,
            (Token::Bytes(bytes), Some(Open::Tag(2 | 3))) if bytes.len() <= SHORT_BIGNUM => {
                around - 1
            }
            _ => around,
        }
    }

    fn opened(mut self, open: Open) -> Self {
        self.open.push(open);
        self
    }

    /// The walk once an item is complete, and with it every container it completes.
    fn completed(mut self) -> Option<Self> {
        loop {
            match self.open.last_mut() {
                None => {
                    self.items += 1;
                    return Some(self);
                }
                Some(Open::Items(left)) if *left > 1 => {
                    *left -= 1;
                    return Some(self);
                }
                Some(Open::Items(_) | Open::Tag(_)) => {
                    self.open.pop();
                }
                Some(Open::UntilBreak | Open::Chunks) => return Some(self),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CIBORIUM_DEPTH, agree, depth};

    fn nested(open: &[u8], levels: usize) -> Vec<u8> {
        [open.repeat(levels), vec![0x00]].concat()
    }

    fn closed(open: &[u8], levels: usize, close: &[u8]) -> Vec<u8> {
        [open.repeat(levels), vec![0x00], close.repeat(levels)].concat()
    }

    /// One test per input: [`depth`] gives `$depth`.
    macro_rules! depth_is {
        ($($name:ident: $input:expr, $depth:expr;)*) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(depth(&$input), $depth);
                }
            )*
        };
    }

    /// One test per input: [`agree`] holds, every oracle reading it as `Tokens` does.
    macro_rules! agrees {
        ($($name:ident: $input:expr;)*) => {
            $(
                #[test]
                fn $name() {
                    agree(&$input);
                }
            )*
        };
    }

    depth_is! {
        a_leaf_is_no_level: *b"\x00", Some(0);
        each_array_is_a_level: *b"\x82\x00\x81\x00", Some(2);
        a_map_and_a_tag_are_each_a_level: *b"\xa1\x00\xc6\x00", Some(2);
        an_indefinite_array_is_a_level: *b"\x9f\x81\x00\xff", Some(2);
        each_of_300_tags_is_a_level: nested(b"\xc6", 300), Some(300);
        an_empty_array_is_a_level: *b"\x80", Some(1);
        an_empty_map_and_an_empty_array_are_each_a_level: *b"\x82\xa0\x81\x80", Some(3);
        an_indefinite_byte_string_is_no_level: *b"\x81\x5f\x41\x00\xff", Some(1);
        an_indefinite_text_is_no_level: *b"\x7f\x61a\xff", Some(0);
        a_short_bignum_innermost_is_no_level: [b"\x81".repeat(CIBORIUM_DEPTH), b"\xc2\x40".to_vec()].concat(), Some(CIBORIUM_DEPTH);
        nothing_has_no_depth: *b"", None;
        a_sequence_has_no_depth: *b"\x00\x00", None;
        an_item_cut_short_has_no_depth: *b"\x82\x00", None;
        a_lone_break_has_no_depth: *b"\xff", None;
    }

    agrees! {
        ciborium_reads_tags_nested_to_its_limit: nested(b"\xc6", CIBORIUM_DEPTH);
        ciborium_reads_arrays_nested_to_its_limit: nested(b"\x81", CIBORIUM_DEPTH);
        ciborium_reads_indefinite_arrays_nested_to_its_limit: closed(b"\x9f", CIBORIUM_DEPTH, b"\xff");
        ciborium_refuses_tags_nested_one_past_its_limit: nested(b"\xc6", CIBORIUM_DEPTH + 1);
        ciborium_refuses_arrays_nested_one_past_its_limit: nested(b"\x81", CIBORIUM_DEPTH + 1);
        ciborium_refuses_tags_nested_far_past_its_limit: nested(b"\xc6", 4 * CIBORIUM_DEPTH);
        ciborium_refuses_arrays_nested_far_past_its_limit: nested(b"\x81", 4 * CIBORIUM_DEPTH);
        ciborium_reads_an_empty_array_innermost_at_its_limit: [b"\x81".repeat(CIBORIUM_DEPTH - 1), b"\x80".to_vec()].concat();
        ciborium_reads_an_empty_map_innermost_at_its_limit: [b"\x81".repeat(CIBORIUM_DEPTH - 1), b"\xa0".to_vec()].concat();
        ciborium_refuses_an_empty_array_innermost_past_its_limit: [b"\x81".repeat(CIBORIUM_DEPTH), b"\x80".to_vec()].concat();
        ciborium_refuses_an_empty_map_innermost_past_its_limit: [b"\x81".repeat(CIBORIUM_DEPTH), b"\xa0".to_vec()].concat();
        ciborium_reads_a_short_bignum_innermost_at_its_limit: [b"\x81".repeat(CIBORIUM_DEPTH), b"\xc2\x40".to_vec()].concat();
        ciborium_reads_a_long_bignum_innermost_as_a_tag: [b"\x81".repeat(CIBORIUM_DEPTH), b"\xc2\x51".to_vec(), vec![0xab; 17]].concat();
        past_its_limit_ciborium_may_refuse_a_simple_value_first: [b"\x82\xe0".as_slice(), &nested(b"\x81", CIBORIUM_DEPTH + 1)].concat();
    }
}
