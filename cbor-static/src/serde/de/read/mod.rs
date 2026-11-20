#![forbid(unsafe_code)]
//! The reads under every generic method of the Deserializer, non-generic or generic over an
//! integer's width only (R-301).

use crate::wire::Major;

pub(super) use any::{Any, Value};

mod any;
mod container;
mod scalar;
mod strings;
mod token;

/// Whose entries a map holds: any keys, or a struct's fields, whose keys are text.
#[derive(Clone, Copy, Debug)]
pub(super) enum Entries {
    Map,
    Struct,
}

/// A map as its keys are read: what remains of it, whose entries it holds, and the encoding of the
/// key before, which under Strict the next must sort after.
#[derive(Debug)]
pub(super) struct Keys<'de> {
    pub(super) remaining: Remaining,
    pub(super) entries: Entries,
    pub(super) previous: &'de [u8],
}

/// What remains of an array or map: a count of items or entries, or items until a break.
#[derive(Clone, Copy, Debug)]
pub(super) enum Remaining {
    Count(u64),
    UntilBreak,
}

/// The initial byte of a break, which ends an indefinite-length item.
const BREAK: u8 = simple(crate::wire::INDEFINITE);
const NULL: u8 = simple(crate::wire::NULL);

/// The initial byte of the head of simple value or break `info`.
const fn simple(info: u8) -> u8 {
    crate::wire::Header {
        major: Major::Simple,
        info,
        arg: 0,
    }
    .initial()
}
