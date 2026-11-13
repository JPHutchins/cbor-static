#![deny(unsafe_code)]
//! CBOR tags (RFC 8949 §3.4) around any value (#12 Q25). A decoder skips no tag but these.

use crate::engine::{DECODE_SELF_DESCRIBED, ENCODE_TAG, dec_tagged, drop_fn};
use crate::lead::Lead;
use crate::schema::{DecKind, Decode, EncKind, Encode, Order};
use crate::stack::Level;
use crate::wire::SELF_DESCRIBE;

/// A value wrapped in CBOR tag `TAG`; any other tag is rejected on decode.
///
/// ```
/// use cbor_static::Tagged;
///
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Point {
///     x: u32,
/// }
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::encode_into(&Tagged::<1000, Point>(Point { x: 1 }), &mut out)?;
/// assert_eq!(&out[..len], b"\xd9\x03\xe8\xa1\x61x\x01");
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(cbor_static::decode_into::<Tagged<1000, Point>>(&out[..len], &mut slot)?.0, Point { x: 1 });
/// assert_eq!(
///     cbor_static::decode_into::<Tagged<1001, Point>>(&out[..len], &mut core::mem::MaybeUninit::uninit())
///         .map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::UnexpectedTag)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Tagged<const TAG: u64, T>(pub T);

/// A standard date/time string (tag 0), not checked against RFC 3339.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let at: &mut cbor_static::DateTime = cbor_static::decode_into(b"\xc0\x6a2013-03-21", &mut slot)?;
/// assert_eq!(at.0, "2013-03-21");
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type DateTime<'de> = Tagged<0, &'de str>;

/// An epoch-based date/time (tag 1), in whatever number type holds it.
///
/// ```
/// let mut out = [0u8; 8];
/// let epoch: cbor_static::Epoch<u64> = cbor_static::Tagged(1_363_896_240);
/// let len = cbor_static::encode_into(&epoch, &mut out)?;
/// assert_eq!(&out[..len], b"\xc1\x1a\x51\x4b\x67\xb0");
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type Epoch<T> = Tagged<1, T>;

/// A value the self-describe tag 55799 precedes when it is encoded. Decoding takes the value with or
/// without the tag.
///
/// ```
/// use cbor_static::SelfDescribed;
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::encode_into(&SelfDescribed(7u32), &mut out)?;
/// assert_eq!(&out[..len], b"\xd9\xd9\xf7\x07");
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(cbor_static::decode_into::<SelfDescribed<u32>>(b"\xd9\xd9\xf7\x07", &mut slot)?.0, 7);
/// assert_eq!(cbor_static::decode_into::<SelfDescribed<u32>>(b"\x07", &mut slot)?.0, 7);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SelfDescribed<T>(pub T);

#[allow(unsafe_code)]
unsafe impl<const TAG: u64, T: Encode> Encode for Tagged<TAG, T> {
    const ENCODE: &'static EncKind = &EncKind::Tagged {
        ops: &ENCODE_TAG,
        inner: T::ENCODE,
        tag: &TAG,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    const ORDER: Order = T::ORDER;
}

#[allow(unsafe_code)]
unsafe impl<'de, const TAG: u64, T: Decode<'de>> Decode<'de> for Tagged<TAG, T> {
    const DECODE: &'static DecKind = &DecKind::Leaf {
        decode: dec_tagged::<TAG, T>,
        default: None,
        drop: drop_fn::<Self>(),
    };
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::tagged(TAG);
}

#[allow(unsafe_code)]
unsafe impl<T: Encode> Encode for SelfDescribed<T> {
    const ENCODE: &'static EncKind = &EncKind::Tagged {
        ops: &ENCODE_TAG,
        inner: T::ENCODE,
        tag: &SELF_DESCRIBE,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    const ORDER: Order = T::ORDER;
}

#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>> Decode<'de> for SelfDescribed<T> {
    const DECODE: &'static DecKind = &DecKind::Described {
        ops: &DECODE_SELF_DESCRIBED,
        inner: T::DECODE,
        drop: drop_fn::<Self>(),
    };
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::TAG.union(T::LEAD);
}
