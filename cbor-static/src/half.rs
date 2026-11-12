#![forbid(unsafe_code)]
//! IEEE 754 binary16 on stable Rust, by its bits (D-22a, D-26).

use crate::engine::{Item, Pending};
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::scalar::{Scalar, floating};
use crate::wire::float;

/// An IEEE 754 binary16 value, held as its bits. It encodes as one `f9` item, and decodes from any
/// float that binary16 holds exactly (D-12). Two are equal when their bits are.
///
/// ```
/// use cbor_static::F16;
///
/// let half = F16::from_f32(1.5).expect("exact");
/// assert_eq!(half.to_bits(), 0x3e00);
/// assert_eq!(half.to_f32(), 1.5);
/// assert_eq!(F16::from_f32(0.1), None);
/// assert_ne!(F16::from_f32(-0.0), F16::from_f32(0.0));
/// assert_eq!(F16::from_bits(0x7e01), F16::from_bits(0x7e01));
///
/// let mut out = [0u8; 3];
/// assert_eq!(cbor_static::encode_into(&half, &mut out)?, 3);
/// assert_eq!(&out, b"\xf9\x3e\x00");
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(*cbor_static::decode_into::<F16>(b"\xfa\x3f\xc0\x00\x00", &mut slot)?, half);
/// assert_eq!(
///     cbor_static::decode_into::<F16>(b"\xfa\x3d\xcc\xcc\xcd", &mut slot).map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::OutOfRange)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct F16(u16);

impl F16 {
    /// ```
    /// assert_eq!(cbor_static::F16::from_bits(0x3c00).to_f32(), 1.0);
    /// ```
    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// ```
    /// assert_eq!(cbor_static::F16::from_f32(-2.0).map(|half| half.to_bits()), Some(0xc000));
    /// ```
    #[must_use]
    pub const fn to_bits(self) -> u16 {
        self.0
    }

    /// The same value as binary32, NaN payloads included.
    ///
    /// ```
    /// use cbor_static::F16;
    ///
    /// assert_eq!(F16::from_bits(0x0001).to_f32().to_bits(), 0x3380_0000);
    /// assert_eq!(F16::from_bits(0x7e01).to_f32().to_bits(), 0x7fc0_2000);
    /// ```
    #[must_use]
    pub fn to_f32(self) -> f32 {
        f32::from_bits(float::half_to_single(self.0))
    }

    /// `value`, when binary16 holds it exactly.
    ///
    /// ```
    /// use cbor_static::F16;
    ///
    /// assert_eq!(F16::from_f32(65504.0).map(F16::to_bits), Some(0x7bff));
    /// assert_eq!(F16::from_f32(65520.0), None);
    /// assert_eq!(F16::from_f32(f32::from_bits(0x7fc0_2000)).map(F16::to_bits), Some(0x7e01));
    /// assert_eq!(F16::from_f32(f32::from_bits(0x7fc0_0001)), None);
    /// ```
    #[must_use]
    pub fn from_f32(value: f32) -> Option<Self> {
        float::to_half(float::from_single(value.to_bits())).map(Self)
    }
}

impl Scalar for F16 {
    const LEAD: Lead = Lead::FLOAT;

    fn encode(self, out: &mut Pending) {
        out.float((float::HALF, self.0.into()));
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        float::to_half(floating(item)?)
            .map(Self)
            .ok_or(ErrorKind::OutOfRange)
    }
}

/// The primitive, through its bits only: the casts would call `__aeabi_h2f` and `__aeabi_f2h`.
#[cfg(feature = "nightly")]
impl Scalar for f16 {
    const LEAD: Lead = Lead::FLOAT;

    fn encode(self, out: &mut Pending) {
        F16(self.to_bits()).encode(out);
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        F16::decode(item).map(|half| Self::from_bits(half.0))
    }
}
