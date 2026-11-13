#![expect(
    clippy::cast_ptr_alignment,
    reason = "every field pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use super::chunks;
use super::text::utf8;
use super::{Item, Next, Pending};
use crate::error::ErrorKind;
use crate::scalar::Scalar;
use crate::schema::{DecKind, Decode, EncKind};
use crate::wire::{Header, Major, NULL};

pub type EncodeFn = unsafe fn(*const u8, &mut Pending);
pub type AbsentFn = unsafe fn(*const u8) -> bool;
pub type DecodeFn = unsafe fn(*mut u8, &Item<'_>) -> Result<Next, ErrorKind>;

pub static ENC_STR: EncKind = EncKind::Leaf {
    encode: enc_str,
    absent: None,
};
pub static DEC_STR: DecKind = DecKind::Leaf {
    decode: dec_str,
    default: None,
    drop: None,
};
pub static ENC_BYTES: EncKind = EncKind::Leaf {
    encode: enc_bytes,
    absent: None,
};
pub static DEC_BYTES: DecKind = DecKind::Leaf {
    decode: dec_bytes,
    default: None,
    drop: None,
};
/// A `skip`ped field (#13 Q31), which is never written: its encoding is always absent.
pub static ENC_SKIPPED: EncKind = EncKind::Leaf {
    encode: enc_nothing,
    absent: Some(always),
};

fn enc_nothing(_: *const u8, _: &mut Pending) {}

const fn always(_: *const u8) -> bool {
    true
}

/// A `skip`ped field's decoding: no key names it, so only an array with an element too many reaches
/// it; its container writes its default instead.
pub fn dec_skipped(_: *mut u8, _: &Item<'_>) -> Result<Next, ErrorKind> {
    Err(ErrorKind::TypeMismatch)
}

/// Only null: what an `Option`'s `None` decodes through.
pub static NULL_ITEM: DecKind = DecKind::Leaf {
    decode: dec_null,
    default: None,
    drop: None,
};
static U128_DIGITS: DecKind = DecKind::Leaf {
    decode: dec_u128_digits,
    default: None,
    drop: None,
};
static I128_POSITIVE: DecKind = DecKind::Leaf {
    decode: dec_i128_positive,
    default: None,
    drop: None,
};
static I128_NEGATIVE: DecKind = DecKind::Leaf {
    decode: dec_i128_negative,
    default: None,
    drop: None,
};

pub unsafe fn enc_scalar<T: Scalar>(src: *const u8, out: &mut Pending) {
    unsafe { src.cast::<T>().read() }.encode(out);
}

pub unsafe fn dec_scalar<T: Scalar>(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    T::decode(item).map(unsafe { store::<T>(dst) })
}

pub unsafe fn enc_option<T: Scalar>(src: *const u8, out: &mut Pending) {
    match unsafe { src.cast::<Option<T>>().read() } {
        Some(value) => value.encode(out),
        None => out.head(Major::Simple, NULL.into()),
    }
}

pub unsafe fn absent_option<T>(src: *const u8) -> bool {
    unsafe { (*src.cast::<Option<T>>()).is_none() }
}

pub unsafe fn dec_option<T: Scalar>(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    match item.header {
        Header {
            major: Major::Simple,
            info: NULL,
            ..
        } => Ok(None),
        _ => T::decode(item).map(Some),
    }
    .map(unsafe { store::<Option<T>>(dst) })
}

pub unsafe fn none<T>(dst: *mut u8) {
    unsafe { dst.cast::<Option<T>>().write(None) };
}

/// Tag `TAG`, after which the same slot takes a `T`.
pub unsafe fn dec_tagged<'de, const TAG: u64, T: Decode<'de>>(
    _: *mut u8,
    item: &Item<'_>,
) -> Result<Next, ErrorKind> {
    match item.header {
        Header {
            major: Major::Tag,
            arg,
            ..
        } if arg == TAG => Ok(Next::Then(T::DECODE)),
        _ => Err(ErrorKind::UnexpectedTag),
    }
}

pub unsafe fn enc_u128(src: *const u8, out: &mut Pending) {
    bignum(out, Major::Uint, 2, unsafe { src.cast::<u128>().read() });
}

pub unsafe fn enc_i128(src: *const u8, out: &mut Pending) {
    match unsafe { src.cast::<i128>().read() } {
        value @ 0.. => bignum(out, Major::Uint, 2, value.cast_unsigned()),
        value => bignum(out, Major::Nint, 3, (!value).cast_unsigned()),
    }
}

pub unsafe fn dec_u128(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    match item.header {
        Header {
            major: Major::Uint,
            arg,
            ..
        } => Ok(unsafe { store::<u128>(dst) }(arg.into())),
        Header {
            major: Major::Tag,
            arg: 2,
            ..
        } => Ok(Next::Then(&U128_DIGITS)),
        Header {
            major: Major::Nint, ..
        }
        | Header {
            major: Major::Tag,
            arg: 3,
            ..
        } => Err(ErrorKind::OutOfRange),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

pub unsafe fn dec_i128(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    match item.header {
        Header {
            major: Major::Uint,
            arg,
            ..
        } => Ok(i128::from(arg)),
        Header {
            major: Major::Nint,
            arg,
            ..
        } => Ok(!i128::from(arg)),
        Header {
            major: Major::Tag,
            arg: 2,
            ..
        } => return Ok(Next::Then(&I128_POSITIVE)),
        Header {
            major: Major::Tag,
            arg: 3,
            ..
        } => return Ok(Next::Then(&I128_NEGATIVE)),
        _ => Err(ErrorKind::TypeMismatch),
    }
    .map(unsafe { store::<i128>(dst) })
}

unsafe fn dec_u128_digits(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    digits(item).map(|stored| unsafe { store_then::<u128>(dst, stored) })
}

unsafe fn dec_i128_positive(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    digits(item)
        .and_then(|(n, next)| Ok((i128::try_from(n).map_err(|_| ErrorKind::OutOfRange)?, next)))
        .map(|stored| unsafe { store_then::<i128>(dst, stored) })
}

unsafe fn dec_i128_negative(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    digits(item)
        .and_then(|(n, next)| Ok((!i128::try_from(n).map_err(|_| ErrorKind::OutOfRange)?, next)))
        .map(|stored| unsafe { store_then::<i128>(dst, stored) })
}

unsafe fn dec_null(_: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    match item.header {
        Header {
            major: Major::Simple,
            info: NULL,
            ..
        } => Ok(Next::Done),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

unsafe fn enc_str(src: *const u8, out: &mut Pending) {
    let text = unsafe { src.cast::<&str>().read() };
    out.head(Major::Text, u64::try_from(text.len()).unwrap_or(u64::MAX));
    out.tail(text.as_bytes());
}

unsafe fn dec_str(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    match (item.header.major, item.header.indefinite()) {
        (Major::Text, false) => utf8(item.payload).map(unsafe { store::<&str>(dst) }),
        (Major::Text, true) => Err(ErrorKind::IndefiniteString),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

unsafe fn enc_bytes(src: *const u8, out: &mut Pending) {
    let bytes = unsafe { src.cast::<&[u8]>().read() };
    out.head(Major::Bytes, u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    out.tail(bytes);
}

unsafe fn dec_bytes(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    match (item.header.major, item.header.indefinite()) {
        (Major::Bytes, false) => Ok(unsafe { store::<&[u8]>(dst) }(item.payload)),
        (Major::Bytes, true) => Err(ErrorKind::IndefiniteString),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn bignum(out: &mut Pending, major: Major, tag: u64, magnitude: u128) {
    if let Ok(small) = u64::try_from(magnitude) {
        out.head(major, small);
    } else {
        let be = magnitude.to_be_bytes();
        let digits = be
            .get(usize::try_from(magnitude.leading_zeros() / 8).unwrap_or(0)..)
            .unwrap_or(&be);
        out.head(Major::Tag, tag);
        out.head(Major::Bytes, u64::try_from(digits.len()).unwrap_or(16));
        out.bytes(digits);
    }
}

/// A bignum's magnitude, from digits in a definite or an indefinite byte string, and how the
/// decoder goes on after them.
fn digits(item: &Item<'_>) -> Result<(u128, Next), ErrorKind> {
    let (digits, next) = chunks::of::<[u8]>(item)?;
    if let Some(strict) = item.strict {
        (strict.bignum)(item.payload)?;
    }
    let significant = digits
        .into_iter()
        .flatten()
        .copied()
        .skip_while(|&b| b == 0);
    if significant.clone().count() > 16 {
        return Err(ErrorKind::OutOfRange);
    }
    Ok((significant.fold(0, |acc, b| acc << 8 | u128::from(b)), next))
}

/// # Safety
/// As [`store`].
unsafe fn store_then<T>(dst: *mut u8, (value, next): (T, Next)) -> Next {
    unsafe { dst.cast::<T>().write(value) };
    next
}

/// # Safety
/// `dst` is valid for an aligned write of `T`.
pub(super) unsafe fn store<T>(dst: *mut u8) -> impl FnOnce(T) -> Next {
    move |value| {
        unsafe { dst.cast::<T>().write(value) };
        Next::Done
    }
}
