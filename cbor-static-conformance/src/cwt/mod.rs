//! CWT (RFC 8392): the claims set, carried as the payload of a COSE message, optionally under
//! the CWT tag. Keys are COSE keys. No cryptography.
//!
//! ```
//! use cbor_static_conformance::cose::{BytesOrNil, Message};
//! use cbor_static_conformance::cwt::{Claims, Tagged};
//!
//! let maced = include_bytes!(concat!(env!("RFC_VECTORS"), "/rfc8392/appendix_a_4.cbor"));
//! let cbor_static::Tagged(Message::Mac0(mac0)) = cbor_static::from_slice::<Tagged>(maced)? else {
//!     panic!("RFC 8392 A.4 is a COSE_Mac0 under the CWT tag")
//! };
//! let BytesOrNil::Bytes(payload) = mac0.payload else { panic!("A.4 has a payload") };
//! assert_eq!(cbor_static::from_slice::<Claims>(payload)?.sub, Some("erikw"));
//! # Ok::<(), cbor_static::Error>(())
//! ```

use cbor_static::{Cbor, RawEntry};
use heapless::Vec;

use crate::cose::Message;

/// The most claims a [`Claims`] holds beyond those it names.
pub(crate) const OTHER_CLAIMS: usize = 8;

/// The most recipients an [`Audience`] names.
pub(crate) const AUDIENCES: usize = 4;

/// RFC 8392 §6's CWT tag over a COSE message. The tag is optional, and an untagged CWT decodes
/// as the bare [`Message`]. One untagged enum cannot hold both forms: it takes a tag-dispatched
/// enum such as [`Message`] to start with any tag, so the CWT tag collides with it.
///
/// ```compile_fail,E0080
/// use cbor_static::Cbor;
/// use cbor_static_conformance::cose::Message;
///
/// #[derive(Cbor)]
/// #[serde(untagged)]
/// enum EitherForm<'a> {
///     Tagged(#[serde(borrow)] cbor_static::Tagged<61, Message<'a>>),
///     Untagged(Message<'a>),
/// }
///
/// let _ = cbor_static::from_slice::<EitherForm>(b"\xd8\x3d\xd2\x84\x40\xa0\xf6\x40");
/// ```
///
/// ```
/// use cbor_static_conformance::cose::Message;
/// use cbor_static_conformance::cwt::Tagged;
///
/// let tagged: Tagged = cbor_static::from_slice(b"\xd8\x3d\xd2\x84\x40\xa0\xf6\x40")?;
/// assert!(matches!(tagged.0, Message::Sign1(_)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type Tagged<'a> = cbor_static::Tagged<61, Message<'a>>;

/// RFC 8392 §3's claims set: the claims §4 registers, and every other claim as written.
///
/// ```
/// use cbor_static_conformance::cwt::{Claims, NumericDate};
///
/// let claims: Claims = cbor_static::from_slice(b"\xa3\x02\x61\x61\x06\x01\x08\xa0")?;
/// assert_eq!((claims.sub, claims.iat, claims.other.len()), (Some("a"), Some(NumericDate::Int(1)), 1));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
pub struct Claims<'a> {
    #[cbor(key = 1, default, skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub iss: Option<&'a str>,
    #[cbor(key = 2, default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<&'a str>,
    #[cbor(key = 3, default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<Audience<'a>>,
    #[cbor(key = 4, default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<NumericDate>,
    #[cbor(key = 5, default, skip_serializing_if = "Option::is_none")]
    pub nbf: Option<NumericDate>,
    #[cbor(key = 6, default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<NumericDate>,
    #[cbor(key = 7, default, skip_serializing_if = "Option::is_none")]
    pub cti: Option<&'a [u8]>,
    #[cbor(other)]
    pub other: Vec<RawEntry<'a>, OTHER_CLAIMS>,
}

/// RFC 8392 §3.1.3's audience: one `StringOrURI`, or an array of them.
///
/// ```
/// use cbor_static_conformance::cwt::Audience;
///
/// assert_eq!(cbor_static::from_slice::<Audience>(b"\x61a")?, Audience::One("a"));
/// let Audience::Many(many) = cbor_static::from_slice(b"\x82\x61a\x61b")? else { panic!("an array") };
/// assert_eq!(many, ["a", "b"]);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[serde(untagged)]
pub enum Audience<'a> {
    One(&'a str),
    Many(#[serde(borrow)] Vec<&'a str, AUDIENCES>),
}

/// RFC 8392 §2's `NumericDate`: seconds since the epoch, an integer or a float, without tag 1.
///
/// ```
/// use cbor_static_conformance::cwt::NumericDate;
///
/// assert_eq!(cbor_static::from_slice::<NumericDate>(b"\x1a\x56\x12\xae\xb0")?, NumericDate::Int(1_444_064_944));
/// assert_eq!(cbor_static::from_slice::<NumericDate>(b"\xf9\x3e\x00")?, NumericDate::Float(1.5));
/// assert!(cbor_static::from_slice::<NumericDate>(b"\xc1\x00").is_err());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
pub enum NumericDate {
    Int(i64),
    Float(f64),
}
