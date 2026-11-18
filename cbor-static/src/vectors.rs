#![deny(unsafe_code)]
//! Encoding into a vector the encoding itself sizes (R-203), behind feature `alloc`.

use alloc::alloc::{Allocator, Global};
use alloc::vec::Vec;

use crate::engine::zeroed;
use crate::error::Error;
use crate::schema::Encode;
use crate::sizes::encoded_len;

/// `value`'s encoding, in a vector of exactly its length.
///
/// ```
/// #[derive(cbor_static::Encode)]
/// struct Reading<'a> {
///     id: u32,
///     name: &'a str,
/// }
///
/// let encoded = cbor_static::to_vec(&Reading { id: 7, name: "a" })?;
/// assert_eq!(encoded, b"\xa2\x62id\x07\x64name\x61a");
/// assert_eq!(encoded.capacity(), encoded.len());
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// As [`to_vec_in`].
pub fn to_vec<T: Encode>(value: &T) -> Result<Vec<u8>, Error> {
    to_vec_in(value, Global)
}

/// [`to_vec`], in a vector from `alloc`.
///
/// ```
/// use core::alloc::{AllocError, Allocator, Layout};
/// use core::ptr::NonNull;
///
/// struct Refusing;
///
/// unsafe impl Allocator for Refusing {
///     fn allocate(&self, _: Layout) -> Result<NonNull<[u8]>, AllocError> {
///         Err(AllocError)
///     }
///
///     unsafe fn deallocate(&self, _: NonNull<u8>, _: Layout) {}
/// }
///
/// assert_eq!(cbor_static::to_vec_in(&"hi", std::alloc::Global)?, b"\x62hi");
/// assert_eq!(
///     cbor_static::to_vec_in(&"hi", Refusing).map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::OutOfMemory)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Errors
/// [`crate::ErrorKind::OutOfMemory`] when `alloc` refuses the vector, and
/// [`crate::ErrorKind::LimitExceeded`] when a field is longer than its `#[cbor(max_len = N)]`.
pub fn to_vec_in<T: Encode, A: Allocator>(value: &T, alloc: A) -> Result<Vec<u8, A>, Error> {
    let mut out = Vec::new_in(alloc);
    let written = crate::to_slice(
        value,
        zeroed(&mut out, encoded_len(value)?).map_err(|kind| Error::new(kind, 0, 0))?,
    )?;
    out.truncate(written);
    Ok(out)
}
