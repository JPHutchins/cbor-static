use cbor_static::Cbor;
use heapless::Vec;

use crate::cose::HeaderMap;
use crate::cwt::Claims;

/// The most cipher suites `SUITES_I` lists.
pub(crate) const SUITES: usize = 8;

/// RFC 9528 Appendix C.2's `suites = [ 2* int ] / int`: the Initiator's suites, the selected one
/// last. The array's minimum of two is not checked.
///
/// ```
/// use cbor_static_conformance::edhoc::Suites;
///
/// assert_eq!(cbor_static::from_slice::<Suites>(b"\x02")?, Suites::One(2));
/// let Suites::Many(suites) = cbor_static::from_slice(b"\x82\x06\x02")? else { panic!("an array") };
/// assert_eq!(suites, [6, 2]);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Suites {
    One(i64),
    Many(Vec<i64, SUITES>),
}

/// RFC 9528 §3.3.2's connection identifier, `bstr / -24..23`. Neither the range nor §3.3.2's rule
/// that a byte string of one such integer's encoding is written as the integer is checked.
///
/// ```
/// use cbor_static_conformance::edhoc::ConnectionId;
///
/// assert_eq!(cbor_static::from_slice::<ConnectionId>(b"\x2d")?, ConnectionId::Int(-14));
/// assert_eq!(cbor_static::from_slice::<ConnectionId>(b"\x41\x18")?, ConnectionId::Bytes(b"\x18"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum ConnectionId<'a> {
    Int(i64),
    Bytes(&'a [u8]),
}

/// RFC 9528 §3.5.3's `ID_CRED_x`, `header_map / bstr / -24..23`: the map as written, or the
/// compact form of a lone `kid`. Whether the compact form is used where §3.5.3.2 requires it is
/// not checked.
///
/// ```
/// use cbor_static_conformance::cose::Label;
/// use cbor_static_conformance::edhoc::IdCred;
///
/// let IdCred::Map(map) = cbor_static::from_slice(b"\xa1\x04\x41\x32")? else { panic!("a map") };
/// assert_eq!(map.get(&Label::Int(4)).map(|kid| kid.as_bytes()), Some(b"\x41\x32".as_slice()));
/// assert_eq!(cbor_static::from_slice::<IdCred>(b"\x32")?, IdCred::Int(-19));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[serde(untagged)]
#[expect(
    clippy::large_enum_variant,
    reason = "the crate is no_std without alloc, so a variant cannot be boxed"
)]
pub enum IdCred<'a> {
    Map(#[serde(borrow)] HeaderMap<'a>),
    Bytes(&'a [u8]),
    Int(i64),
}

/// RFC 9528 §3.5.2's `CRED_x` as the traces carry it: an X.509 certificate's DER in a byte
/// string, or a CWT Claims Set (CCS).
///
/// ```
/// use cbor_static_conformance::edhoc::Cred;
///
/// let Cred::Ccs(ccs) = cbor_static::from_slice(b"\xa1\x02\x61\x61")? else { panic!("a CCS") };
/// assert_eq!(ccs.sub, Some("a"));
/// assert_eq!(cbor_static::from_slice::<Cred>(b"\x41\x30")?, Cred::Certificate(b"\x30"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[serde(untagged)]
#[expect(
    clippy::large_enum_variant,
    reason = "the crate is no_std without alloc, so a variant cannot be boxed"
)]
pub enum Cred<'a> {
    Certificate(&'a [u8]),
    Ccs(#[serde(borrow)] Claims<'a>),
}
