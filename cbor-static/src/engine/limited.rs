//! The per-type side of `#[cbor(max_len = N)]` (#5 Q15): the length check encoding runs before a
//! value, and the decoders and copies that stop at `N`.

use core::ptr;

use super::{DecodeFn, Item, Next, leaf};
use crate::error::ErrorKind;
use crate::limits::Limit;
use crate::wire::{Header, Major};

/// The value at `at`, or null when it is longer than `N`.
///
/// # Safety
/// `at` points to a valid `T`.
pub unsafe fn limited<T: Limit, const N: usize>(at: *const u8) -> *const u8 {
    if unsafe { &*at.cast::<T>() }.length() <= N {
        at
    } else {
        ptr::null()
    }
}

/// Whether a `major` item's declared length is at most `N`. An indefinite one declares none, and
/// an item of another major type is its form's to reject.
///
/// # Errors
/// [`ErrorKind::LimitExceeded`] for a longer one.
pub fn fits<const N: usize>(header: Header, major: Major) -> Result<(), ErrorKind> {
    if header.major != major
        || header.indefinite()
        || usize::try_from(header.arg).is_ok_and(|len| len <= N)
    {
        Ok(())
    } else {
        Err(ErrorKind::LimitExceeded)
    }
}

/// A borrowed text string of at most `N` bytes.
pub const fn str_limited<const N: usize>() -> DecodeFn {
    dec_limited::<N, true>
}

/// A borrowed byte string of at most `N` bytes.
pub const fn bytes_limited<const N: usize>() -> DecodeFn {
    dec_limited::<N, false>
}

unsafe fn dec_limited<const N: usize, const TEXT: bool>(
    dst: *mut u8,
    item: &Item<'_>,
) -> Result<Next, ErrorKind> {
    fits::<N>(item.header, if TEXT { Major::Text } else { Major::Bytes })?;
    if TEXT {
        unsafe { leaf::dec_str(dst, item) }
    } else {
        unsafe { leaf::dec_bytes(dst, item) }
    }
}

/// Whether a segment `at` bytes into a string keeps it within `N` bytes.
#[cfg(feature = "heapless")]
fn within<const N: usize>(at: usize, segment: &[u8]) -> Result<(), ErrorKind> {
    match at.checked_add(segment.len()) {
        Some(end) if end <= N => Ok(()),
        _ => Err(ErrorKind::LimitExceeded),
    }
}

#[cfg(feature = "heapless")]
pub use bounded::{append_string_limited, append_vec_limited, at_limited};

#[cfg(feature = "heapless")]
mod bounded {
    use core::ptr;

    use heapless::LenType;

    use super::within;
    use crate::engine::{append_string, append_vec, at};
    use crate::error::ErrorKind;

    /// # Safety
    /// As [`append_string`].
    pub unsafe fn append_string_limited<const CAP: usize, L: LenType, const N: usize>(
        dst: *mut u8,
        at: usize,
        segment: &[u8],
    ) -> Result<(), ErrorKind> {
        within::<N>(at, segment)?;
        unsafe { append_string::<CAP, L>(dst, at, segment) }
    }

    /// # Safety
    /// As [`append_vec`].
    pub unsafe fn append_vec_limited<const CAP: usize, L: LenType, const N: usize>(
        dst: *mut u8,
        at: usize,
        segment: &[u8],
    ) -> Result<(), ErrorKind> {
        within::<N>(at, segment)?;
        unsafe { append_vec::<CAP, L>(dst, at, segment) }
    }

    /// [`at`], with no slot past element `N`.
    ///
    /// # Safety
    /// As [`at`].
    pub unsafe fn at_limited<T, const CAP: usize, L: LenType, const N: usize>(
        vec: *mut u8,
        len: usize,
    ) -> *mut u8 {
        let slot = unsafe { at::<T, CAP, L>(vec, len) };
        if len < N { slot } else { ptr::null_mut() }
    }
}
