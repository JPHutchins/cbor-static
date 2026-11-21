use cbor_static::{Cbor, RawEntry};
use heapless::Vec;

use super::header::{ENTRIES, Label};

/// The most keys a [`KeySet`] holds.
pub(crate) const KEYS: usize = 4;

/// RFC 9052 §7's `COSE_Key`: the common parameters, and the key type's own as written.
///
/// ```
/// use cbor_static_conformance::cose::{Key, Label};
///
/// let input = b"\xa4\x01\x02\x20\x01\x21\x41\x00\x22\x41\x01";
/// let key: Key = cbor_static::from_slice(input)?;
/// assert_eq!((key.kty, key.parameters.len()), (Label::Int(2), 3));
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&key, &mut out)?;
/// assert_eq!(&out[..len], input);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
pub struct Key<'a> {
    #[cbor(key = 1)]
    #[serde(borrow)]
    pub kty: Label<'a>,
    #[cbor(key = 2, default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<&'a [u8]>,
    #[cbor(key = 3, default, skip_serializing_if = "Option::is_none")]
    pub alg: Option<Label<'a>>,
    #[cbor(key = 4, default, skip_serializing_if = "Option::is_none")]
    pub key_ops: Option<Vec<Label<'a>, ENTRIES>>,
    #[cbor(key = 5, default, skip_serializing_if = "Option::is_none")]
    pub base_iv: Option<&'a [u8]>,
    #[cbor(other)]
    pub parameters: Vec<RawEntry<'a>, ENTRIES>,
}

/// RFC 9052 §7's `COSE_KeySet`.
///
/// ```
/// use cbor_static_conformance::cose::KeySet;
///
/// let set: KeySet = cbor_static::from_slice(b"\x82\xa1\x01\x04\xa1\x01\x01")?;
/// assert_eq!(set.0.len(), 2);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
pub struct KeySet<'a>(#[serde(borrow)] pub Vec<Key<'a>, KEYS>);
