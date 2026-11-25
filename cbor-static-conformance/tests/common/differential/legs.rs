//! Each oracle against `Tokens` on one item, after the normalization of that oracle's leg alone:
//! what that reader cannot keep is lowered on cbor-static's side too, and what it must refuse is
//! asserted refused.

use core::fmt;

use super::{Data, cbor2, python, readers};

/// A normalization that applied to an item: each is counted, so a run shows which rows it reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Row {
    /// ciborium reads `undefined` as null.
    CiboriumUndefinedIsNull,
    /// ciborium refuses an item holding an unassigned simple value.
    CiboriumRefusesUnassignedSimple,
    /// ciborium reads a tag 2 or 3 over at most 16 definite bytes as its integer.
    CiboriumBignum,
    /// ciborium refuses a tag 3 over 16 bytes whose integer is below `i128::MIN`.
    CiboriumRefusesBignum,
    /// ciborium flattens indefinite lengths and chunked strings.
    CiboriumFlattens,
    /// cbor2 flattens indefinite lengths and chunked strings.
    Cbor2Flattens,
    /// cbor2's dict collapses keys Python finds equal.
    Cbor2CollapsesKeys,
    /// A reader quiets or rewrites a NaN.
    NanIsAnyNan,
}

impl fmt::Display for Row {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// Why ciborium must refuse an item, and the words its error says it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    UnassignedSimple,
    BignumTooLarge,
}

impl Refusal {
    const fn says(self) -> &'static str {
        match self {
            Self::UnassignedSimple => "known simple value",
            Self::BignumTooLarge => "integer too large",
        }
    }

    const fn row(self) -> Row {
        match self {
            Self::UnassignedSimple => Row::CiboriumRefusesUnassignedSimple,
            Self::BignumTooLarge => Row::CiboriumRefusesBignum,
        }
    }
}

/// Asserts that ciborium, minicbor and cbor2 each read `bytes` as `Tokens` does, after their own
/// leg's normalization; returns the rows that applied.
///
/// # Panics
///
/// If `Tokens` refuses the item, or an oracle reads it otherwise.
#[track_caller]
pub fn agree(bytes: &[u8]) -> Vec<Row> {
    let ours = readers::tokens(bytes)
        .unwrap_or_else(|error| panic!("Tokens refuses {bytes:02x?}: {error}"));
    [
        ciborium(bytes, &ours),
        minicbor(bytes, &ours),
        cbor2(bytes, &ours),
    ]
    .concat()
}

/// ciborium's `Value` against `ours`.
#[track_caller]
pub fn ciborium(bytes: &[u8], ours: &Data) -> Vec<Row> {
    let theirs = readers::ciborium(bytes);
    let rows = [
        rows_if(ours.has_undefined(), Row::CiboriumUndefinedIsNull),
        rows_if(ours.has_indefinite(), Row::CiboriumFlattens),
        rows_if(
            theirs.as_ref().is_ok_and(Data::has_any_nan),
            Row::NanIsAnyNan,
        ),
    ]
    .concat();
    match (for_ciborium(ours.clone()), theirs) {
        (Ok(expected), Ok(theirs)) => {
            assert_eq!(theirs, expected.flattened(), "ciborium reads {bytes:02x?}");
            [rows, rows_if(ours.has_short_bignum(), Row::CiboriumBignum)].concat()
        }
        (Err(refusal), Err(error)) => {
            assert!(
                error.contains(refusal.says()),
                "ciborium refuses {bytes:02x?} as {error:?}, not for {refusal:?}"
            );
            [rows, vec![refusal.row()]].concat()
        }
        (Ok(expected), Err(error)) => {
            panic!("ciborium refuses {bytes:02x?} ({error}), expected {expected:?}")
        }
        (Err(refusal), Ok(theirs)) => {
            panic!("ciborium reads {bytes:02x?} as {theirs:?}, expected a refusal for {refusal:?}")
        }
    }
}

/// minicbor's Tokenizer against `ours`, indefinite lengths and chunks included.
#[track_caller]
pub fn minicbor(bytes: &[u8], ours: &Data) -> Vec<Row> {
    let theirs = readers::minicbor(bytes)
        .unwrap_or_else(|error| panic!("minicbor refuses {bytes:02x?}: {error}"));
    assert_eq!(&theirs, ours, "minicbor reads {bytes:02x?}");
    rows_if(theirs.has_any_nan(), Row::NanIsAnyNan)
}

/// cbor2's re-encoding, read back by `Tokens`, against `ours` as the dict cbor2 builds.
#[track_caller]
pub fn cbor2(bytes: &[u8], ours: &Data) -> Vec<Row> {
    let theirs = match cbor2::reply(bytes) {
        cbor2::Reply::Reencoded { cbor } => readers::tokens(&cbor)
            .unwrap_or_else(|error| panic!("Tokens refuses cbor2's {cbor:02x?}: {error}"))
            .nan_is_any_nan(),
        cbor2::Reply::Refused { reason } => panic!("cbor2 refuses {bytes:02x?}: {reason}"),
    };
    let flat = ours.clone().flattened();
    let expected = python::collapsed(flat.clone());
    assert_eq!(theirs, expected, "cbor2 reads {bytes:02x?}");
    [
        rows_if(ours.has_indefinite(), Row::Cbor2Flattens),
        rows_if(expected != flat, Row::Cbor2CollapsesKeys),
        rows_if(theirs.has_any_nan(), Row::NanIsAnyNan),
    ]
    .concat()
}

/// `ours` as ciborium's `Value` holds it, or why ciborium must refuse it: the first refusal in
/// reading order, as ciborium stops at it.
pub fn for_ciborium(ours: Data) -> Result<Data, Refusal> {
    match ours {
        Data::Undefined => Ok(Data::Null),
        Data::Simple(_) => Err(Refusal::UnassignedSimple),
        Data::Tag(tag @ (2 | 3), content) => match *content {
            Data::Bytes(bytes) if bytes.len() <= 16 => bignum(tag, &bytes),
            content => Ok(Data::Tag(tag, Box::new(for_ciborium(content)?))),
        },
        Data::Tag(tag, content) => Ok(Data::Tag(tag, Box::new(for_ciborium(*content)?))),
        Data::Array(items, length) => Ok(Data::Array(
            items
                .into_iter()
                .map(for_ciborium)
                .collect::<Result<_, _>>()?,
            length,
        )),
        Data::Map(entries, length) => Ok(Data::Map(
            entries
                .into_iter()
                .map(|(key, value)| Ok((for_ciborium(key)?, for_ciborium(value)?)))
                .collect::<Result<_, _>>()?,
            length,
        )),
        leaf => Ok(leaf),
    }
}

/// ciborium's reading of tag 2 or 3 over at most 16 bytes: the integer where `ciborium::Value`'s
/// integer holds it, else the tag over the magnitude's bytes without leading zeros; a tag 3 whose
/// integer is below `i128::MIN` is refused.
fn bignum(tag: u64, bytes: &[u8]) -> Result<Data, Refusal> {
    let magnitude = bytes
        .iter()
        .fold(0u128, |sum, &byte| sum << 8 | u128::from(byte));
    match (tag, u64::try_from(magnitude)) {
        (2, Ok(small)) => Ok(Data::Integer(small.into())),
        (_, Ok(small)) => Ok(Data::Integer(-1 - i128::from(small))),
        (3, Err(_)) if i128::try_from(magnitude).is_err() => Err(Refusal::BignumTooLarge),
        (_, Err(_)) => Ok(Data::Tag(
            tag,
            Box::new(Data::Bytes(
                bytes
                    .iter()
                    .copied()
                    .skip_while(|&byte| byte == 0)
                    .collect(),
            )),
        )),
    }
}

fn rows_if(applies: bool, row: Row) -> Vec<Row> {
    if applies { vec![row] } else { vec![] }
}
