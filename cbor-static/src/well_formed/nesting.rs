//! R-113 (D-14a, D-14b): a generated, deterministic nesting corpus. The skip state's limits are
//! checked at and one past 64 open indefinite levels and 8 saved counts. Misplaced breaks and every
//! short sequence over a small alphabet of heads are checked against the reference.

use core::iter::successors;
use std::vec;
use std::vec::Vec;

use super::{Class, agree, chunk_sizes, oracle, skipped, streamed, tokens};
use crate::Tokens;
use crate::engine::item_len;
use crate::error::ErrorKind;

const ALPHABET: [u8; 12] = [
    0x00, 0x21, 0x41, 0x61, 0x80, 0x81, 0x82, 0x9f, 0xa1, 0xbf, 0xc1, 0xff,
];

/// `open` repeated `depth` times, then `inner`, then `close` repeated `depth` times.
fn nested(open: &[u8], depth: usize, inner: &[u8], close: &[u8]) -> Vec<u8> {
    [open.repeat(depth), inner.to_vec(), close.repeat(depth)].concat()
}

/// Every walker's verdict on `bytes`: the error kind when it fails.
fn kinds(bytes: &[u8]) -> [Result<usize, ErrorKind>; 3] {
    [
        item_len(bytes),
        Tokens::new(bytes)
            .try_for_each(|token| token.map(|_| ()))
            .map(|()| bytes.len())
            .map_err(|e| e.kind),
        streamed(bytes, 7),
    ]
}

#[test]
fn indefinite_levels_up_to_64_and_one_past() {
    for (open, inner, close) in [
        (b"\x9f".as_slice(), b"".as_slice(), b"\xff".as_slice()),
        (b"\xbf\x01", b"\x80", b"\xff"),
        (b"\x9f\x01", b"", b"\xff"),
    ] {
        let at_limit = nested(open, 64, inner, close);
        assert_eq!(kinds(&at_limit), [Ok(at_limit.len()); 3], "{open:02x?}");
        assert_eq!(oracle::item(&at_limit), Ok(at_limit.len()));
        let past = nested(open, 65, inner, close);
        assert_eq!(
            kinds(&past),
            [Err(ErrorKind::LimitExceeded); 3],
            "{open:02x?}"
        );
        assert_eq!(oracle::item(&past), Ok(past.len()));
    }
}

#[test]
fn saved_counts_up_to_8_and_one_past() {
    for (open, close) in [
        (b"\x82\x9f".as_slice(), b"\xff\x00".as_slice()),
        (b"\xa1\x9f", b"\xff\x00"),
        (b"\xbf\x9f", b"\xff\x00\xff"),
    ] {
        let at_limit = nested(open, 8, b"", close);
        assert_eq!(kinds(&at_limit), [Ok(at_limit.len()); 3], "{open:02x?}");
        let past = nested(open, 9, b"", close);
        assert_eq!(
            kinds(&past),
            [Err(ErrorKind::LimitExceeded); 3],
            "{open:02x?}"
        );
        assert_eq!(oracle::item(&past), Ok(past.len()));
    }
}

#[test]
fn definite_nesting_has_no_limit() {
    let depth = if cfg!(miri) { 300 } else { 100_000 };
    for (open, inner) in [
        (b"\x81".as_slice(), b"\x00".as_slice()),
        (b"\xa1\x01", b"\x00"),
    ] {
        let deep = nested(open, depth, inner, b"");
        assert_eq!(skipped(&deep), Ok(deep.len()));
        assert_eq!(tokens(&deep), Ok(()));
        assert_eq!(streamed(&deep, 64), Ok(deep.len()));
    }
}

#[test]
fn misplaced_breaks_agree_with_the_reference() {
    for well_formed in [
        b"\x82\x9f\x01\xff\x02".as_slice(),
        b"\xa2\x61a\x9f\x01\x02\xff\x61b\xbf\x01\x82\x03\x04\xff",
        b"\x9f\xbf\x9f\xff\x01\xff\x81\x9f\xff\xff",
        b"\x7f\x61a\x62bc\xff",
        b"\xc1\x9f\xd8\x20\x5f\x41\x00\xff\xff",
    ] {
        assert_eq!(oracle::item(well_formed), Ok(well_formed.len()));
        agree(well_formed, chunk_sizes(well_formed.len()));
        for at in 0..=well_formed.len() {
            let inserted = [&well_formed[..at], b"\xff", &well_formed[at..]].concat();
            agree(&inserted, [1, 3, inserted.len()]);
        }
        for at in 0..well_formed.len() {
            let mut replaced = well_formed.to_vec();
            replaced[at] = 0xff;
            agree(&replaced, [1, 3, replaced.len()]);
        }
    }
}

#[test]
fn every_short_sequence_of_heads_agrees_with_the_reference() {
    let longest = if cfg!(miri) { 2 } else { 5 };
    let sequences: Vec<Vec<u8>> = successors(Some(vec![Vec::new()]), |shorter| {
        Some(
            shorter
                .iter()
                .flat_map(|prefix| {
                    ALPHABET
                        .iter()
                        .map(|&byte| [prefix.as_slice(), &[byte]].concat())
                })
                .collect(),
        )
    })
    .take(longest + 1)
    .flatten()
    .collect();
    let rejected = sequences
        .iter()
        .filter(|bytes| {
            agree(bytes, [1, bytes.len().max(1)]);
            oracle::item(bytes).is_err()
        })
        .count();
    assert!(rejected > 0 && rejected < sequences.len());
}

#[test]
fn the_reference_classifies_like_the_rfc() {
    assert_eq!(oracle::item(b"\xbf\x00\xff"), Err(Class::Malformed));
    assert_eq!(oracle::item(b"\x62\xc3\x28"), Err(Class::InvalidUtf8));
    assert_eq!(oracle::item(b"\x9f\x01"), Err(Class::Truncated));
    assert_eq!(oracle::item(b"\x01\x02"), Ok(1));
    assert_eq!(oracle::sequence(b"\x01\x02"), Ok(()));
}
