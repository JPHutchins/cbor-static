#![deny(unsafe_code)]
//! Leaf field types: each maps to engine handlers reached only through its own descriptor (D-22).

use crate::engine::{
    DEC_BYTES, DEC_STR, ENC_BYTES, ENC_STR, Item, Pending, absent_option, dec_i128, dec_option,
    dec_scalar, dec_tagged, dec_u128, enc_i128, enc_option, enc_scalar, enc_tagged, enc_u128, none,
};
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::schema::{DecKind, Decode, EncKind, Encode};
use crate::stack::{Level, depth_of};
use crate::wire::{FALSE, Major, TRUE, float};

/// A single-item value with no nesting and no borrowed payload.
pub trait Scalar: Copy + 'static {
    const ENCODE_KIND: &'static EncKind = &EncKind::Leaf {
        encode: enc_scalar::<Self>,
        absent: None,
    };
    const DECODE_KIND: &'static DecKind = &DecKind::Leaf {
        decode: dec_scalar::<Self>,
        default: None,
        drop: None,
    };

    const LEAD: Lead;

    fn encode(self, out: &mut Pending);

    /// # Errors
    /// A type or range mismatch.
    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind>;
}

/// A value wrapped in CBOR tag `TAG`; any other tag is rejected on decode.
///
/// ```
/// use cbor_static::Tagged;
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::encode_into(&Tagged::<1, u64>(1_363_896_240), &mut out)?;
/// assert_eq!(&out[..len], b"\xc1\x1a\x51\x4b\x67\xb0");
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(cbor_static::decode_into::<Tagged<1, u64>>(&out[..len], &mut slot)?.0, 1_363_896_240);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Tagged<const TAG: u64, T>(pub T);

impl Scalar for u32 {
    const LEAD: Lead = Lead::UINT;

    fn encode(self, out: &mut Pending) {
        out.head(Major::Uint, self.into());
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        Self::try_from(unsigned(item)?).map_err(|_| ErrorKind::OutOfRange)
    }
}

impl Scalar for u64 {
    const LEAD: Lead = Lead::UINT;

    fn encode(self, out: &mut Pending) {
        out.head(Major::Uint, self);
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        unsigned(item)
    }
}

impl Scalar for i32 {
    const LEAD: Lead = Lead::UINT.union(Lead::NINT);

    fn encode(self, out: &mut Pending) {
        match i64::from(self) {
            value @ 0.. => out.head(Major::Uint, value.cast_unsigned()),
            value => out.head(Major::Nint, (!value).cast_unsigned()),
        }
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        Self::try_from(signed(item)?).map_err(|_| ErrorKind::OutOfRange)
    }
}

impl Scalar for bool {
    const LEAD: Lead = Lead::BOOL;

    fn encode(self, out: &mut Pending) {
        out.head(Major::Simple, if self { TRUE } else { FALSE }.into());
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        match (item.header.major, item.header.info) {
            (Major::Simple, FALSE) => Ok(false),
            (Major::Simple, TRUE) => Ok(true),
            _ => Err(ErrorKind::TypeMismatch),
        }
    }
}

impl Scalar for f64 {
    const LEAD: Lead = Lead::FLOAT;

    fn encode(self, out: &mut Pending) {
        out.float(float::shortest(self));
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        floating(item)
    }
}

impl Scalar for f32 {
    const LEAD: Lead = Lead::FLOAT;

    fn encode(self, out: &mut Pending) {
        out.float(float::shortest(self.into()));
    }

    fn decode(item: &Item<'_>) -> Result<Self, ErrorKind> {
        float::to_f32(floating(item)?).ok_or(ErrorKind::OutOfRange)
    }
}

fn unsigned(item: &Item<'_>) -> Result<u64, ErrorKind> {
    match item.header.major {
        Major::Uint => Ok(item.header.arg),
        Major::Nint => Err(ErrorKind::OutOfRange),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn signed(item: &Item<'_>) -> Result<i128, ErrorKind> {
    match item.header.major {
        Major::Uint => Ok(item.header.arg.into()),
        Major::Nint => Ok(!i128::from(item.header.arg)),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn floating(item: &Item<'_>) -> Result<f64, ErrorKind> {
    match item.header.major {
        Major::Simple => {
            float::value(item.header.info, item.header.arg).ok_or(ErrorKind::TypeMismatch)
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

/// A scalar's `Option` is itself a leaf: no frame, `None` as null.
macro_rules! scalar {
	($($ty:ty),+) => {$(
		#[allow(unsafe_code)]
		unsafe impl Encode for $ty {
			const ENCODE: &'static EncKind = <$ty as Scalar>::ENCODE_KIND;
			type Stack = ();
			const MAX_DEPTH: usize = 0;
			const OPTION_ENCODE: &'static EncKind = &EncKind::Leaf {
				encode: enc_option::<$ty>,
				absent: Some(absent_option::<$ty>),
			};
		}

		#[allow(unsafe_code)]
		unsafe impl<'de> Decode<'de> for $ty {
			const DECODE: &'static DecKind = <$ty as Scalar>::DECODE_KIND;
			type Stack = ();
			const MAX_DEPTH: usize = 0;
			const OPTION_DECODE: &'static DecKind = &DecKind::Leaf {
				decode: dec_option::<$ty>,
				default: Some(none::<$ty>),
				drop: None,
			};
			type OptionStack = ();
			const LEAD: Lead = <$ty as Scalar>::LEAD;
		}
	)+};
}

/// Any other leaf's `Option` decodes through one frame.
macro_rules! leaf {
    ($ty:ty, $lead:expr, $encode:expr, $decode:expr) => {
        #[allow(unsafe_code)]
        unsafe impl Encode for $ty {
            const ENCODE: &'static EncKind = $encode;
            type Stack = ();
            const MAX_DEPTH: usize = 0;
        }

        #[allow(unsafe_code)]
        unsafe impl<'de> Decode<'de> for $ty {
            const DECODE: &'static DecKind = $decode;
            type Stack = ();
            const MAX_DEPTH: usize = 0;
            type OptionStack = Level<()>;
            const LEAD: Lead = $lead;
        }
    };
}

scalar!(u32, u64, i32, bool, f32, f64);
leaf!(
    u128,
    Lead::UINT.union(Lead::TAG),
    &EncKind::Leaf {
        encode: enc_u128,
        absent: None
    },
    &DecKind::Leaf {
        decode: dec_u128,
        default: None,
        drop: None
    }
);
leaf!(
    i128,
    Lead::UINT.union(Lead::NINT).union(Lead::TAG),
    &EncKind::Leaf {
        encode: enc_i128,
        absent: None
    },
    &DecKind::Leaf {
        decode: dec_i128,
        default: None,
        drop: None
    }
);

#[allow(unsafe_code)]
unsafe impl Encode for &str {
    const ENCODE: &'static EncKind = &ENC_STR;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

#[allow(unsafe_code)]
unsafe impl<'de: 'a, 'a> Decode<'de> for &'a str {
    const DECODE: &'static DecKind = &DEC_STR;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::TEXT;
}

#[allow(unsafe_code)]
unsafe impl Encode for &[u8] {
    const ENCODE: &'static EncKind = &ENC_BYTES;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

#[allow(unsafe_code)]
unsafe impl<'de: 'a, 'a> Decode<'de> for &'a [u8] {
    const DECODE: &'static DecKind = &DEC_BYTES;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::BYTES;
}

#[allow(unsafe_code)]
unsafe impl<T: Encode> Encode for Option<T> {
    const ENCODE: &'static EncKind = T::OPTION_ENCODE;
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
}

#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>> Decode<'de> for Option<T> {
    const DECODE: &'static DecKind = T::OPTION_DECODE;
    type Stack = T::OptionStack;
    const MAX_DEPTH: usize = depth_of::<T::OptionStack>();
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::NULL.union(T::LEAD);
}

#[allow(unsafe_code)]
unsafe impl<const TAG: u64, T: Scalar> Encode for Tagged<TAG, T> {
    const ENCODE: &'static EncKind = &EncKind::Leaf {
        encode: enc_tagged::<TAG, T>,
        absent: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

#[allow(unsafe_code)]
unsafe impl<const TAG: u64, T: Scalar> Decode<'_> for Tagged<TAG, T> {
    const DECODE: &'static DecKind = &DecKind::Leaf {
        decode: dec_tagged::<TAG, T>,
        default: None,
        drop: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::TAG;
}
