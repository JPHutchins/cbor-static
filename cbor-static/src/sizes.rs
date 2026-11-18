#![deny(unsafe_code)]
//! Bounds on a type's encoding (R-207), and the encode whose buffer the compiler sizes from them
//! (R-201).

use crate::engine::{Encoder, Progress, Skipped};
use crate::error::{Error, ErrorKind};
use crate::half::F16;
use crate::schema::Encode;
use crate::tags::{Embedded, SelfDescribed, Tagged};
use crate::wire::{ENCODED_CBOR, Head, Major, SELF_DESCRIBE};

/// The most bytes a value of this type encodes to: the longest of its preferred encodings
/// (RFC 8949 §4.1), so `u64` counts 9 and `f64` counts 9 though most values take fewer.
///
/// ```
/// use cbor_static::MaxEncodedLen;
///
/// assert_eq!(<(u8, i64) as MaxEncodedLen>::MAX_ENCODED_LEN, 12);
/// assert_eq!(cbor_static::encoded_len(&(u8::MAX, i64::MIN))?, 12);
/// assert_eq!(<Option<[bool; 3]> as MaxEncodedLen>::MAX_ENCODED_LEN, 4);
/// assert_eq!(<cbor_static::Tagged<1000, f32> as MaxEncodedLen>::MAX_ENCODED_LEN, 8);
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// A type whose values have no longest encoding, such as `&str`, has none:
///
/// ```compile_fail,E0277
/// let _ = <&str as cbor_static::MaxEncodedLen>::MAX_ENCODED_LEN;
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no maximum encoded length",
    label = "unbounded",
    note = "a field bounds an unbounded type with `#[cbor(max_len = N)]`"
)]
pub trait MaxEncodedLen {
    const MAX_ENCODED_LEN: usize;
}

/// The most bytes a `#[cbor(bytes)]` field of this type holds.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no maximum encoded length",
    label = "unbounded",
    note = "a field bounds an unbounded type with `#[cbor(max_len = N)]`"
)]
pub trait ByteCapacity {
    const CAPACITY: usize;
}

/// Encodes `value` into `out`, which the compiler checks holds `T::MAX_ENCODED_LEN` bytes, and
/// returns the bytes written.
///
/// ```
/// let mut out = [0u8; 9];
/// let len = cbor_static::encode_into(&(true, 500u16), &mut out)?;
/// assert_eq!(&out[..len], b"\x82\xf5\x19\x01\xf4");
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// A smaller buffer does not compile:
///
/// ```compile_fail,E0080
/// let mut out = [0u8; 4];
/// let _ = cbor_static::encode_into(&(true, 500u16), &mut out);
/// ```
///
/// # Errors
/// [`crate::ErrorKind::LimitExceeded`] when a field is longer than its `#[cbor(max_len = N)]`.
pub fn encode_into<T: Encode + MaxEncodedLen, const N: usize>(
    value: &T,
    out: &mut [u8; N],
) -> Result<usize, Error> {
    const {
        assert!(
            N >= T::MAX_ENCODED_LEN,
            "cbor-static: the buffer is shorter than the type's MAX_ENCODED_LEN"
        );
    };
    crate::to_slice(value, out)
}

/// The length of `value`'s encoding, which it counts by encoding `value` into a small buffer on
/// the stack.
///
/// ```
/// assert_eq!(cbor_static::encoded_len(&"twenty-four characters!!")?, 26);
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`crate::to_slice`].
pub fn encoded_len<T: Encode>(value: &T) -> Result<usize, Error> {
    counted(Encoder::new(value))
}

/// The length of the encoding `encoder` writes, which it counts in a small buffer on the stack.
pub(crate) fn counted<T: Encode>(mut encoder: Encoder<'_, T>) -> Result<usize, Error> {
    let mut scratch = [0u8; 64];
    let mut counted = 0;
    loop {
        match encoder.resume(&mut scratch)? {
            Progress::NeedSpace(0) => {
                return Err(Error::new(ErrorKind::BufferTooSmall, 0, counted));
            }
            Progress::NeedSpace(written) => counted += written,
            Progress::Done(written) => return Ok(counted + written),
        }
    }
}

/// The length of a head whose argument is `arg`, whatever its major type.
#[doc(hidden)]
#[must_use]
pub const fn head_len(arg: u64) -> usize {
    Head::new(Major::Uint, arg).as_bytes().len()
}

/// The longest encoding of a container of `count` items, each at most `each` bytes long.
#[doc(hidden)]
#[must_use]
pub const fn container_len(count: usize, each: usize) -> usize {
    head_len(count as u64) + count * each
}

#[doc(hidden)]
#[must_use]
pub const fn larger(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

macro_rules! fixed {
    ($($ty:ty),+) => {$(
        /// A head carrying the whole value, or a float's widest preferred width.
        impl MaxEncodedLen for $ty {
            const MAX_ENCODED_LEN: usize = 1 + size_of::<$ty>();
        }
    )+};
}

fixed!(
    u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64, F16
);
#[cfg(feature = "nightly")]
fixed!(f16);

impl MaxEncodedLen for () {
    const MAX_ENCODED_LEN: usize = 1;
}

impl MaxEncodedLen for bool {
    const MAX_ENCODED_LEN: usize = 1;
}

/// A bignum's tag, then a byte string of up to 16 bytes.
impl MaxEncodedLen for u128 {
    const MAX_ENCODED_LEN: usize = head_len(2) + container_len(size_of::<Self>(), 1);
}

impl MaxEncodedLen for i128 {
    const MAX_ENCODED_LEN: usize = <u128 as MaxEncodedLen>::MAX_ENCODED_LEN;
}

impl MaxEncodedLen for char {
    const MAX_ENCODED_LEN: usize = container_len(char::MAX.len_utf8(), 1);
}

impl<T: MaxEncodedLen> MaxEncodedLen for Option<T> {
    const MAX_ENCODED_LEN: usize =
        larger(<() as MaxEncodedLen>::MAX_ENCODED_LEN, T::MAX_ENCODED_LEN);
}

impl<T: MaxEncodedLen, const N: usize> MaxEncodedLen for [T; N] {
    const MAX_ENCODED_LEN: usize = container_len(N, T::MAX_ENCODED_LEN);
}

impl<const TAG: u64, T: MaxEncodedLen> MaxEncodedLen for Tagged<TAG, T> {
    const MAX_ENCODED_LEN: usize = head_len(TAG) + T::MAX_ENCODED_LEN;
}

impl<T: MaxEncodedLen> MaxEncodedLen for SelfDescribed<T> {
    const MAX_ENCODED_LEN: usize = head_len(SELF_DESCRIBE) + T::MAX_ENCODED_LEN;
}

impl<T: MaxEncodedLen> MaxEncodedLen for Embedded<T> {
    const MAX_ENCODED_LEN: usize = head_len(ENCODED_CBOR) + container_len(T::MAX_ENCODED_LEN, 1);
}

/// As a map's `other` field, it writes no entries.
///
/// ```
/// assert_eq!(<cbor_static::Skipped as cbor_static::MaxEncodedLen>::MAX_ENCODED_LEN, 0);
/// ```
impl MaxEncodedLen for Skipped {
    const MAX_ENCODED_LEN: usize = 0;
}

macro_rules! tuple {
    ($count:literal: $($T:ident),+) => {
        impl<$($T: MaxEncodedLen),+> MaxEncodedLen for ($($T,)+) {
            const MAX_ENCODED_LEN: usize = head_len($count) $(+ $T::MAX_ENCODED_LEN)+;
        }
    };
}

tuple!(1: A);
tuple!(2: A, B);
tuple!(3: A, B, C);
tuple!(4: A, B, C, D);

#[cfg(feature = "heapless")]
mod bounded {
    use heapless::{LenType, LinearMap, String, Vec};

    use super::{MaxEncodedLen, container_len};

    impl<const N: usize, L: LenType> MaxEncodedLen for String<N, L> {
        const MAX_ENCODED_LEN: usize = container_len(N, 1);
    }

    impl<T: MaxEncodedLen, const N: usize, L: LenType> MaxEncodedLen for Vec<T, N, L> {
        const MAX_ENCODED_LEN: usize = container_len(N, T::MAX_ENCODED_LEN);
    }

    impl<K: MaxEncodedLen, V: MaxEncodedLen, const N: usize> MaxEncodedLen for LinearMap<K, V, N> {
        const MAX_ENCODED_LEN: usize = container_len(N, K::MAX_ENCODED_LEN + V::MAX_ENCODED_LEN);
    }
}
