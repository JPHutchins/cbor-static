#![deny(unsafe_code)]
//! `Bounded<T, N>`, a `T` whose `len()` is at most `N`, which encodes and decodes as `T` under
//! `#[cbor(max_len = N)]` (#5 Q15). It bounds a type wherever it is nested, where the attribute
//! bounds only its own field.

use core::ops::Deref;

use crate::lead::Lead;
use crate::limits::{Limit, Limited, LimitedBytes, LimitedDecode, encode_limited};
use crate::schema::{ByteString, DecKind, Decode, EncKind, Encode, Order, sealed};
use crate::sizes::{ByteCapacity, MaxEncodedLen};
use crate::stack::Level;

/// A `T` of at most `N` elements, bytes or entries.
///
/// ```
/// use cbor_static::{Bounded, ErrorKind, MaxEncodedLen};
///
/// let names = vec![Bounded::<String, 4>::new("ab".into()).expect("fits")];
/// assert_eq!(Bounded::<String, 4>::new("abcde".into()), Err("abcde".into()));
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::to_slice(&names, &mut out)?;
/// assert_eq!(&out[..len], b"\x81\x62ab");
///
/// assert_eq!(cbor_static::from_slice::<Vec<Bounded<String, 4>>>(&out[..len])?, names);
/// assert_eq!(
///     cbor_static::from_slice::<Vec<Bounded<String, 4>>>(b"\x81\x65abcde").map_err(|e| e.kind),
///     Err(ErrorKind::LimitExceeded)
/// );
/// assert_eq!(<Bounded<Vec<u16>, 2> as MaxEncodedLen>::MAX_ENCODED_LEN, 7);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Bounded<T, const N: usize>(T);

impl<T: Limit, const N: usize> Bounded<T, N> {
    /// `value`, or `value` back when it is longer than `N`.
    ///
    /// ```
    /// use cbor_static::Bounded;
    ///
    /// assert_eq!(Bounded::<Vec<u8>, 2>::new(vec![1, 2]).map(Bounded::into_inner), Ok(vec![1, 2]));
    /// assert_eq!(Bounded::<Vec<u8>, 2>::new(vec![1, 2, 3]), Err(vec![1, 2, 3]));
    /// ```
    ///
    /// # Errors
    /// `value` itself, when its `len()` is over `N`.
    pub fn new(value: T) -> Result<Self, T> {
        if value.length() <= N {
            Ok(Self(value))
        } else {
            Err(value)
        }
    }
}

impl<T, const N: usize> Bounded<T, N> {
    /// The `T` it holds.
    ///
    /// ```
    /// let name = cbor_static::Bounded::<String, 8>::new("ab".into()).map_err(|_| ())?;
    /// assert_eq!(name.into_inner(), "ab");
    /// # Ok::<(), ()>(())
    /// ```
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// ```
/// let name = cbor_static::Bounded::<String, 8>::new("ab".into()).map_err(|_| ())?;
/// assert_eq!(name.len(), 2);
/// # Ok::<(), ()>(())
/// ```
impl<T, const N: usize> Deref for Bounded<T, N> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

#[allow(unsafe_code)]
unsafe impl<T: Encode + Limit, const N: usize> Encode for Bounded<T, N> {
    const ENCODE: &'static EncKind = &encode_limited::<T, N>(T::ENCODE);
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
    const ORDER: Order = T::ORDER;
}

#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>, const N: usize> Decode<'de> for Bounded<T, N>
where
    Limited<T, N>: LimitedDecode<'de>,
{
    const DECODE: &'static DecKind = <Limited<T, N> as LimitedDecode<'de>>::DECODE;
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = T::LEAD;
    const CONTIGUOUS: bool = T::CONTIGUOUS;
}

impl<T, const N: usize> MaxEncodedLen for Bounded<T, N>
where
    Limited<T, N>: MaxEncodedLen,
{
    const MAX_ENCODED_LEN: usize = <Limited<T, N> as MaxEncodedLen>::MAX_ENCODED_LEN;
}

impl<T: ByteString, const N: usize> sealed::ByteString for Bounded<T, N> {}

/// `#[cbor(bytes)]`: the byte string `T` is, of at most `N` bytes.
///
/// ```
/// use cbor_static::{Bounded, MaxEncodedLen};
///
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// #[cbor(max_encoded_len)]
/// struct Frame {
///     #[cbor(bytes)]
///     payload: Bounded<Vec<u8>, 64>,
/// }
///
/// assert_eq!(<Frame as MaxEncodedLen>::MAX_ENCODED_LEN, 75);
/// let frame: Frame = cbor_static::from_slice(b"\xa1\x67payload\x42\x01\x02")?;
/// assert_eq!(*frame.payload, [1, 2]);
/// assert_eq!(
///     cbor_static::from_slice::<Frame>(&[b"\xa1\x67payload\x58\x41".as_slice(), &[0; 65]].concat())
///         .map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::LimitExceeded)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
impl<T: ByteString + Limit, const N: usize> ByteString for Bounded<T, N>
where
    Limited<T, N>: LimitedBytes,
{
    const ENCODE_BYTES: &'static EncKind = &encode_limited::<T, N>(T::ENCODE_BYTES);
    const DECODE_BYTES: &'static DecKind = <Limited<T, N> as LimitedBytes>::DECODE_BYTES;
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
}

impl<T: ByteString, const N: usize> ByteCapacity for Bounded<T, N> {
    const CAPACITY: usize = N;
}
