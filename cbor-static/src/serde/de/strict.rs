#![forbid(unsafe_code)]
//! [`super::Deserializer::new_strict`] (#6 Q4): input only as RFC 8949 §4.2.1's deterministic
//! encoding writes it, through checks reached only from [`STRICT`], which only `new_strict` names,
//! so firmware without it links none of them (D-22).

use crate::ErrorKind;
use crate::wire::Header;

/// One check per place Strict looks, each rejecting with [`ErrorKind::NotDeterministic`].
#[derive(Debug)]
pub(super) struct StrictOps {
    /// Every head read: the native Strict's own check.
    pub(super) head: fn(&[u8]) -> Result<Header, ErrorKind>,
    /// A bignum's magnitude, from this many digits: the native Strict's own check.
    pub(super) bignum: fn(u128, usize) -> Result<(), ErrorKind>,
    /// The key a map's input starts with, after the key before it.
    pub(super) order: for<'de> fn(&[u8], &'de [u8]) -> Key<'de>,
    /// A whole item's every head, and the key order of its every map: the native Strict's own
    /// check.
    pub(super) raw: fn(&[u8]) -> Result<(), ErrorKind>,
}

/// A map's key as Strict measures it: its encoding, and whether that sorts after the key before.
pub(super) struct Key<'de> {
    pub(super) encoding: &'de [u8],
    pub(super) rises: bool,
}

pub(super) static STRICT: StrictOps = StrictOps {
    head: crate::engine::strict::head,
    bignum: crate::engine::strict::bignum,
    order,
    raw: crate::engine::strict::raw,
};

/// The key `rest` starts with, which must sort strictly after `previous`, bytewise. A key not
/// whole in `rest` is left for reading it to refuse.
fn order<'de>(previous: &[u8], rest: &'de [u8]) -> Key<'de> {
    match crate::engine::item_len(rest)
        .ok()
        .and_then(|len| rest.get(..len))
    {
        Some(encoding) => Key {
            encoding,
            rises: previous < encoding,
        },
        None => Key {
            encoding: &[],
            rises: true,
        },
    }
}
