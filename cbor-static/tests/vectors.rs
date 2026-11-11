//! R-133 (D-27, D-27a): cbor-wg/cbor-test-vectors, pinned as a submodule at 7e84843 and read from
//! the `.cbor` twins of its EDN files. Floats compare bitwise, so NaN payloads count.

use std::mem::MaybeUninit;

use cbor_static::{ErrorKind, Tagged, Token, TokenWriter, Tokens, decode_into};

macro_rules! vectors {
    ($($name:literal: $path:literal),+ $(,)?) => {
        [$(($name, include_bytes!(concat!("../../tests/fixtures/cbor-test-vectors/tests/", $path)).as_slice())),+]
    };
}

const GOOD: [(&str, &[u8]); 11] = vectors![
    "mt0": "rfc8949-appendixA/mt0.cbor",
    "mt1": "rfc8949-appendixA/mt1.cbor",
    "mt2": "rfc8949-appendixA/mt2.cbor",
    "mt3": "rfc8949-appendixA/mt3.cbor",
    "mt4": "rfc8949-appendixA/mt4.cbor",
    "mt5": "rfc8949-appendixA/mt5.cbor",
    "mt6": "rfc8949-appendixA/mt6.cbor",
    "mt7-float": "rfc8949-appendixA/mt7-float.cbor",
    "mt7-simple": "rfc8949-appendixA/mt7-simple.cbor",
    "good": "rfc8949/good.cbor",
    "spike": "spike/spike.cbor",
];
const BAD: &[u8] = include_bytes!("../../tests/fixtures/cbor-test-vectors/tests/rfc8949/bad.cbor");

/// Well-formed items with invalid tag content: tag 1 and tag 0 around the map `{"a": 0}`. Tag
/// validity is a typed decode's job (D-12), not the token decoder's.
const WELL_FORMED_BAD: [&[u8]; 2] = [b"\xc1\xa1\x61a\x00", b"\xc0\xa1\x61a\x00"];

struct Vector<'de> {
    description: &'de str,
    encoded: &'de [u8],
    decoded: &'de [Token<'de>],
    roundtrip: bool,
}

#[test]
fn every_file_parses_into_its_tests() {
    let counts: Vec<(&str, usize)> = GOOD
        .iter()
        .map(|&(name, file)| (name, vectors(&tokens(file)).len()))
        .chain([("bad", vectors(&tokens(BAD)).len())])
        .collect();
    assert_eq!(
        counts,
        [
            ("mt0", 11),
            ("mt1", 5),
            ("mt2", 2),
            ("mt3", 7),
            ("mt4", 4),
            ("mt5", 5),
            ("mt6", 8),
            ("mt7-float", 22),
            ("mt7-simple", 6),
            ("good", 88),
            ("spike", 1165),
            ("bad", 47),
        ]
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "1,165 spike vectors; the Appendix A and good files run instead"
)]
fn encoded_and_decoded_are_the_same_tokens_in_every_file() {
    assert_eq!(collapsed_streams(&GOOD), [("spike", 366)]);
}

#[test]
fn encoded_and_decoded_are_the_same_tokens_in_appendix_a_and_good() {
    assert_eq!(collapsed_streams(&GOOD[..10]), []);
}

#[test]
#[cfg_attr(
    miri,
    ignore = "1,165 spike vectors; the Appendix A and good files run instead"
)]
fn the_writer_reproduces_every_preferred_encoding() {
    assert_eq!(reproduced(&GOOD), 693);
}

#[test]
fn the_writer_reproduces_preferred_encodings_in_appendix_a_and_good() {
    assert_eq!(reproduced(&GOOD[..10]), 132);
}

#[test]
fn every_bad_vector_is_rejected_by_the_layer_whose_rule_it_breaks() {
    let tokens = tokens(BAD);
    let well_formed: Vec<&[u8]> = vectors(&tokens)
        .iter()
        .filter(|vector| Tokens::new(vector.encoded).all(|token| token.is_ok()))
        .map(|vector| vector.encoded)
        .collect();
    assert_eq!(well_formed, WELL_FORMED_BAD);
    assert_eq!(
        typed::<Tagged<1, u64>>(WELL_FORMED_BAD[0]),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(
        typed::<Tagged<0, u64>>(WELL_FORMED_BAD[1]),
        Err(ErrorKind::TypeMismatch)
    );
}

/// For each file whose stream comparison needed its own `collapseBigInts` decode option, the
/// number of vectors it changed. Every comparison must hold.
fn collapsed_streams<'a>(files: &[(&'a str, &[u8])]) -> Vec<(&'a str, usize)> {
    files
        .iter()
        .filter_map(|&(name, file)| {
            let tokens = tokens(file);
            let collapse = option(&tokens, "decodeOptions", "collapseBigInts");
            let collapsed = vectors(&tokens)
                .iter()
                .filter(|vector| {
                    let encoded = self::tokens(vector.encoded);
                    let compared = if collapse {
                        bignums_collapsed(&encoded)
                    } else {
                        encoded.clone()
                    };
                    assert_eq!(
                        bitwise(&compared),
                        bitwise(vector.decoded),
                        "{name}: {description}",
                        description = vector.description
                    );
                    bitwise(&compared) != bitwise(&encoded)
                })
                .count();
            (collapsed > 0).then_some((name, collapsed))
        })
        .collect()
}

/// Re-encodes every vector not marked `roundtrip: false`, asserts that it reproduces `encoded`,
/// and counts them.
fn reproduced(files: &[(&str, &[u8])]) -> usize {
    files
        .iter()
        .map(|&(name, file)| {
            vectors(&tokens(file))
                .iter()
                .filter(|vector| vector.roundtrip)
                .inspect(|vector| {
                    assert_eq!(
                        reencoded(&tokens(vector.encoded)),
                        vector.encoded,
                        "{name}: {description}",
                        description = vector.description
                    );
                })
                .count()
        })
        .sum()
}

fn tokens(bytes: &[u8]) -> Vec<Token<'_>> {
    Tokens::new(bytes)
        .collect::<Result<_, _>>()
        .expect("well-formed")
}

fn reencoded(tokens: &[Token]) -> Vec<u8> {
    let mut out = vec![0; 1 << 16];
    let mut writer = TokenWriter::new(&mut out);
    for &token in tokens {
        writer.write(token).expect("writes");
    }
    let len = writer.written();
    out.truncate(len);
    out
}

fn typed<'de, T: cbor_static::Decode<'de> + Copy>(bytes: &'de [u8]) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    decode_into(bytes, &mut slot)
        .map(|value| *value)
        .map_err(|e| e.kind)
}

/// Floats as their bits, so that NaN payloads and signed zeros compare.
fn bitwise<'de>(tokens: &[Token<'de>]) -> Vec<Result<Token<'de>, u64>> {
    tokens
        .iter()
        .map(|&token| match token {
            Token::Float(value) => Err(value.to_bits()),
            other => Ok(other),
        })
        .collect()
}

/// RFC 8949 §3.4.3 bignums as the decoders that wrote the twins see them under `collapseBigInts`:
/// leading zero bytes dropped, and an integer when the rest fits a `u64`.
fn bignums_collapsed<'de>(tokens: &[Token<'de>]) -> Vec<Token<'de>> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut rest = tokens;
    while let Some((&first, tail)) = rest.split_first() {
        if let (Token::Tag(tag @ (2 | 3)), Some(&Token::Bytes(digits))) = (first, tail.first()) {
            let significant = &digits[digits.iter().take_while(|&&b| b == 0).count()..];
            match (significant.len() <= 8, tag) {
                (true, 2) => out.push(Token::Unsigned(be(significant))),
                (true, _) => out.push(Token::Negative(be(significant))),
                (false, _) => out.extend([Token::Tag(tag), Token::Bytes(significant)]),
            }
            rest = &tail[1..];
        } else {
            out.push(first);
            rest = tail;
        }
    }
    out
}

fn be(digits: &[u8]) -> u64 {
    digits.iter().fold(0, |n, &b| n << 8 | u64::from(b))
}

/// The tests of a vector file: a map whose `tests` entry is an array of maps.
fn vectors<'de>(file: &'de [Token<'de>]) -> Vec<Vector<'de>> {
    items(entry(file, "tests").expect("has tests"))
        .into_iter()
        .map(|test| Vector {
            description: match entry(test, "description") {
                Some([Token::Text(text)]) => text,
                other => panic!("description: {other:?}"),
            },
            encoded: match entry(test, "encoded") {
                Some([Token::Bytes(bytes)]) => bytes,
                other => panic!("encoded: {other:?}"),
            },
            decoded: entry(test, "decoded").unwrap_or(&[]),
            roundtrip: entry(test, "roundtrip") != Some(&[Token::Bool(false)]),
        })
        .collect()
}

/// Whether the file-level options map `options` sets `name` to true.
fn option(file: &[Token], options: &str, name: &str) -> bool {
    entry(file, options).and_then(|map| entry(map, name)) == Some(&[Token::Bool(true)])
}

/// The value of the text key `key` in the definite map that `map` is.
fn entry<'a, 'de>(map: &'a [Token<'de>], key: &str) -> Option<&'a [Token<'de>]> {
    items(map)
        .chunks(2)
        .find(|pair| pair.first() == Some(&[Token::Text(key)].as_slice()))
        .and_then(|pair| pair.get(1).copied())
}

/// The items directly inside the definite array or map that `container` is.
fn items<'a, 'de>(container: &'a [Token<'de>]) -> Vec<&'a [Token<'de>]> {
    let (count, inside) = match container.split_first() {
        Some((Token::Array(Some(n)), inside)) => (*n, inside),
        Some((Token::Map(Some(n)), inside)) => (2 * n, inside),
        other => panic!("not a definite container: {other:?}"),
    };
    (0..count)
        .scan(inside, |rest, _| {
            let (item, after) = rest.split_at(item_len(rest));
            *rest = after;
            Some(item)
        })
        .collect()
}

/// The number of tokens the item at the start of `tokens` spans.
fn item_len(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .scan(vec![Some(1_u64)], |open, token| {
            if *token == Token::Break {
                open.pop();
            } else {
                if let Some(Some(left)) = open.last_mut() {
                    *left -= 1;
                }
                match token {
                    Token::Array(Some(n)) => open.push(Some(*n)),
                    Token::Map(Some(n)) => open.push(Some(2 * n)),
                    Token::Tag(_) => open.push(Some(1)),
                    Token::Array(None)
                    | Token::Map(None)
                    | Token::BytesIndefinite
                    | Token::TextIndefinite => open.push(None),
                    _ => {}
                }
            }
            while open.last() == Some(&Some(0)) {
                open.pop();
            }
            Some(open.is_empty())
        })
        .position(|done| done)
        .map_or(tokens.len(), |last| last + 1)
}
