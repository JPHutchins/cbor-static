use cbor_static::Cbor;

use super::header::{Label, Protected};
use super::message::BytesOrNil;

/// RFC 9052 §4.4's `Sig_structure` with its `sign_protected`, for `COSE_Signature` and
/// countersignatures. That element is optional mid-array, which an array struct cannot express,
/// so the two shapes are two types.
///
/// ```
/// use cbor_static_conformance::cose::{Protected, SigContext, SigStructure};
///
/// let structure = SigStructure {
///     context: SigContext::Signature,
///     body_protected: Protected(b""),
///     sign_protected: Protected(b"\xa1\x01\x26"),
///     external_aad: b"",
///     payload: b"x",
/// };
/// let mut out = [0u8; 32];
/// let len = cbor_static::to_slice(&structure, &mut out)?;
/// assert_eq!(cbor_static::from_slice::<SigStructure>(&out[..len])?, structure);
/// assert_eq!(&out[..12], b"\x85\x69Signature\x40");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct SigStructure<'a> {
    pub context: SigContext,
    #[serde(borrow)]
    pub body_protected: Protected<'a>,
    pub sign_protected: Protected<'a>,
    pub external_aad: &'a [u8],
    pub payload: &'a [u8],
}

/// RFC 9052 §4.4's `Sig_structure` without `sign_protected`, for `COSE_Sign1`.
///
/// ```
/// use cbor_static_conformance::cose::{Protected, SigContext, SigStructure1};
///
/// let structure = SigStructure1 {
///     context: SigContext::Signature1,
///     body_protected: Protected(b""),
///     external_aad: b"",
///     payload: b"",
/// };
/// let mut out = [0u8; 32];
/// let len = cbor_static::to_slice(&structure, &mut out)?;
/// assert_eq!(&out[..len], b"\x84\x6aSignature1\x40\x40\x40");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct SigStructure1<'a> {
    pub context: SigContext,
    #[serde(borrow)]
    pub body_protected: Protected<'a>,
    pub external_aad: &'a [u8],
    pub payload: &'a [u8],
}

/// `Sig_structure`'s context: RFC 9052 §4.4's, and RFC 8152 §4.4's and RFC 9338's
/// countersignatures, which cose-wg/Examples still carry.
///
/// ```
/// use cbor_static_conformance::cose::SigContext;
///
/// assert_eq!(cbor_static::from_slice::<SigContext>(b"\x70CounterSignature")?, SigContext::CounterSignature);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigContext {
    Signature,
    Signature1,
    CounterSignature,
    CounterSignature0,
}

/// RFC 9052 §5.3's `Enc_structure`.
///
/// ```
/// use cbor_static_conformance::cose::{EncContext, EncStructure, Protected};
///
/// let structure = EncStructure { context: EncContext::Encrypt0, protected: Protected(b""), external_aad: b"" };
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&structure, &mut out)?;
/// assert_eq!(&out[..len], b"\x83\x68Encrypt0\x40\x40");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct EncStructure<'a> {
    pub context: EncContext,
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub external_aad: &'a [u8],
}

/// `Enc_structure`'s context.
///
/// ```
/// use cbor_static_conformance::cose::EncContext;
///
/// assert_eq!(cbor_static::from_slice::<EncContext>(b"\x6dEnc_Recipient")?, EncContext::EncRecipient);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncContext {
    Encrypt,
    Encrypt0,
    #[serde(rename = "Enc_Recipient")]
    EncRecipient,
    #[serde(rename = "Mac_Recipient")]
    MacRecipient,
    #[serde(rename = "Rec_Recipient")]
    RecRecipient,
}

/// RFC 9052 §6.3's `MAC_structure`.
///
/// ```
/// use cbor_static_conformance::cose::{MacContext, MacStructure, Protected};
///
/// let structure = MacStructure { context: MacContext::Mac0, protected: Protected(b""), external_aad: b"", payload: b"" };
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&structure, &mut out)?;
/// assert_eq!(&out[..len], b"\x84\x64MAC0\x40\x40\x40");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct MacStructure<'a> {
    pub context: MacContext,
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub external_aad: &'a [u8],
    pub payload: &'a [u8],
}

/// `MAC_structure`'s context.
///
/// ```
/// use cbor_static_conformance::cose::MacContext;
///
/// assert_eq!(cbor_static::from_slice::<MacContext>(b"\x63MAC")?, MacContext::Mac);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacContext {
    #[serde(rename = "MAC")]
    Mac,
    #[serde(rename = "MAC0")]
    Mac0,
}

/// RFC 9053 §5.2's `COSE_KDF_Context`.
///
/// ```
/// use cbor_static_conformance::cose::KdfContext;
///
/// let input = b"\x84\x01\x83\xf6\xf6\xf6\x83\xf6\xf6\xf6\x82\x18\x80\x44\xa1\x01\x38\x18";
/// let context: KdfContext = cbor_static::from_slice(input)?;
/// assert_eq!(context.supp_pub_info.key_data_length, 128);
/// let mut out = [0u8; 32];
/// let len = cbor_static::to_slice(&context, &mut out)?;
/// assert_eq!(&out[..len], input);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct KdfContext<'a> {
    #[serde(borrow)]
    pub algorithm_id: Label<'a>,
    pub party_u_info: PartyInfo<'a>,
    pub party_v_info: PartyInfo<'a>,
    pub supp_pub_info: SuppPubInfo<'a>,
    pub supp_priv_info: Option<&'a [u8]>,
}

/// RFC 9053 §5.2's `PartyInfo`.
///
/// ```
/// use cbor_static_conformance::cose::{BytesOrNil, Nonce, PartyInfo};
///
/// let party: PartyInfo = cbor_static::from_slice(b"\x83\x41\x55\x02\xf6")?;
/// assert_eq!((party.identity, party.nonce, party.other), (BytesOrNil::Bytes(b"U"), Nonce::Int(2), BytesOrNil::Nil(())));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct PartyInfo<'a> {
    #[serde(borrow)]
    pub identity: BytesOrNil<'a>,
    pub nonce: Nonce<'a>,
    pub other: BytesOrNil<'a>,
}

/// `PartyInfo`'s nonce, `bstr / int / nil`.
///
/// ```
/// use cbor_static_conformance::cose::Nonce;
///
/// assert_eq!(cbor_static::from_slice::<Nonce>(b"\x20")?, Nonce::Int(-1));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Nonce<'a> {
    Bytes(&'a [u8]),
    Int(i64),
    Nil(()),
}

/// RFC 9053 §5.2's `SuppPubInfo`.
///
/// ```
/// use cbor_static_conformance::cose::SuppPubInfo;
///
/// let info: SuppPubInfo = cbor_static::from_slice(b"\x83\x18\x80\x40\x41\x00")?;
/// assert_eq!(info.other, Some(b"\x00".as_slice()));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Debug, PartialEq)]
#[cbor(array)]
pub struct SuppPubInfo<'a> {
    pub key_data_length: u64,
    #[serde(borrow)]
    pub protected: Protected<'a>,
    pub other: Option<&'a [u8]>,
}
