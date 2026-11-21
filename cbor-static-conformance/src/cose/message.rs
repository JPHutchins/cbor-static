use cbor_static::{Cbor, RawCbor};
use heapless::Vec;

use super::header::{HeaderMap, Protected};

/// The most signatures or recipients one layer holds.
pub(crate) const LAYER: usize = 4;

/// RFC 9052 §2's `COSE_Tagged_Message`.
///
/// ```
/// use cbor_static_conformance::cose::Message;
///
/// let input = include_bytes!(concat!(env!("COSE_WG_VECTORS"), "/RFC8152/Appendix_C_4_1/output.cbor"));
/// assert!(matches!(cbor_static::from_slice::<Message>(input)?, Message::Encrypt0(_)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
pub enum Message<'a> {
    #[cbor(cbor_tag = 98)]
    Sign(#[serde(borrow)] Sign<'a>),
    #[cbor(cbor_tag = 18)]
    Sign1(Sign1<'a>),
    #[cbor(cbor_tag = 96)]
    Encrypt(Encrypt<'a>),
    #[cbor(cbor_tag = 16)]
    Encrypt0(Encrypt0<'a>),
    #[cbor(cbor_tag = 97)]
    Mac(Mac<'a>),
    #[cbor(cbor_tag = 17)]
    Mac0(Mac0<'a>),
}

/// CDDL `bstr / nil`. An `Option` in an array struct's last position would drop a written nil, so the choice is its own type.
///
/// ```
/// use cbor_static_conformance::cose::BytesOrNil;
///
/// assert_eq!(cbor_static::from_slice::<BytesOrNil>(b"\xf6")?, BytesOrNil::Nil(()));
/// assert_eq!(cbor_static::from_slice::<BytesOrNil>(b"\x41\x00")?, BytesOrNil::Bytes(b"\x00"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum BytesOrNil<'a> {
    Nil(()),
    Bytes(&'a [u8]),
}

/// RFC 9052 §4.1's `COSE_Sign`.
///
/// ```
/// use cbor_static_conformance::cose::Sign;
///
/// let input = include_bytes!(concat!(env!("COSE_WG_VECTORS"), "/RFC8152/Appendix_C_1_1/output.cbor"));
/// assert_eq!(cbor_static::from_slice::<Sign>(&input[2..])?.signatures.len(), 1);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Sign<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub payload: BytesOrNil<'a>,
    pub signatures: Vec<Signature<'a>, LAYER>,
}

/// RFC 9052 §4.1's `COSE_Signature`.
///
/// ```
/// use cbor_static_conformance::cose::Signature;
///
/// let signature: Signature = cbor_static::from_slice(b"\x83\x40\xa0\x41\x00")?;
/// assert_eq!(signature.signature, b"\x00");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Signature<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub signature: &'a [u8],
}

/// RFC 9052 §4.2's `COSE_Sign1`.
///
/// ```
/// use cbor_static_conformance::cose::{BytesOrNil, Sign1};
///
/// let sign1: Sign1 = cbor_static::from_slice(b"\x84\x40\xa0\xf6\x41\x00")?;
/// assert_eq!(sign1.payload, BytesOrNil::Nil(()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Sign1<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub payload: BytesOrNil<'a>,
    pub signature: &'a [u8],
}

/// RFC 9052 §5.1's `COSE_Encrypt`.
///
/// ```
/// use cbor_static_conformance::cose::Encrypt;
///
/// let input = include_bytes!(concat!(env!("COSE_WG_VECTORS"), "/RFC8152/Appendix_C_3_1/output.cbor"));
/// assert_eq!(cbor_static::from_slice::<Encrypt>(&input[2..])?.recipients.len(), 1);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Encrypt<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub ciphertext: BytesOrNil<'a>,
    pub recipients: Vec<Recipient<'a, Recipient<'a>>, LAYER>,
}

/// RFC 9052 §5.1's `COSE_recipient`, whose own recipients are `Inner`. The rule is recursive,
/// which a derived type cannot be, so a recipient's recipients are typed to a fixed depth
/// and kept raw below it.
///
/// ```
/// use cbor_static_conformance::cose::{BytesOrNil, Recipient};
///
/// let recipient: Recipient = cbor_static::from_slice(b"\x84\x40\xa0\xf6\x81\x83\x40\xa0\x41\x00")?;
/// assert_eq!(recipient.ciphertext, BytesOrNil::Nil(()));
/// assert_eq!(recipient.recipients.map(|inner| inner[0].as_bytes()), Some(b"\x83\x40\xa0\x41\x00".as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Recipient<'a, Inner = RawCbor<'a>> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub ciphertext: BytesOrNil<'a>,
    pub recipients: Option<Vec<Inner, LAYER>>,
}

/// RFC 9052 §5.2's `COSE_Encrypt0`.
///
/// ```
/// use cbor_static_conformance::cose::{BytesOrNil, Encrypt0};
///
/// let encrypt0: Encrypt0 = cbor_static::from_slice(b"\x83\x40\xa0\xf6")?;
/// assert_eq!(encrypt0.ciphertext, BytesOrNil::Nil(()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Encrypt0<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub ciphertext: BytesOrNil<'a>,
}

/// RFC 9052 §6.1's `COSE_Mac`.
///
/// ```
/// use cbor_static_conformance::cose::Mac;
///
/// let input = include_bytes!(concat!(env!("COSE_WG_VECTORS"), "/RFC8152/Appendix_C_5_1/output.cbor"));
/// assert_eq!(cbor_static::from_slice::<Mac>(&input[2..])?.recipients.len(), 1);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Mac<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub payload: BytesOrNil<'a>,
    pub tag: &'a [u8],
    pub recipients: Vec<Recipient<'a, Recipient<'a>>, LAYER>,
}

/// RFC 9052 §6.2's `COSE_Mac0`.
///
/// ```
/// use cbor_static_conformance::cose::Mac0;
///
/// let mac0: Mac0 = cbor_static::from_slice(b"\x84\x40\xa0\x40\x41\x00")?;
/// assert_eq!(mac0.tag, b"\x00");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct Mac0<'a> {
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub unprotected: HeaderMap<'a>,
    pub payload: BytesOrNil<'a>,
    pub tag: &'a [u8],
}
