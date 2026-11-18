//! The per-type side of the containers that allocate (feature `alloc`, #5 Q9, Q10).
#![expect(
    clippy::cast_ptr_alignment,
    reason = "every container pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use alloc::alloc::Layout;
use alloc::vec::Vec;
use core::ptr;

use super::limited::within;
use crate::error::ErrorKind;

/// Commits the `len` elements before slot `len`, then returns that slot, reserving it first when
/// the vector is full; null when it cannot be reserved.
///
/// # Safety
/// `vec` points to a valid `Vec<T>`, whose first `len` elements, up to its capacity, are
/// initialized.
pub unsafe fn at<T>(vec: *mut u8, len: usize) -> *mut u8 {
    let vec = unsafe { &mut *vec.cast::<Vec<T>>() };
    unsafe { vec.set_len(len) };
    if vec.len() == vec.capacity() && vec.try_reserve(1).is_err() {
        return ptr::null_mut();
    }
    unsafe { vec.as_mut_ptr().add(len) }.cast()
}

/// Appends `item`, reserving its slot first.
///
/// # Errors
/// [`ErrorKind::OutOfMemory`] when the slot cannot be reserved.
pub fn push<T>(vec: &mut Vec<T>, item: T) -> Result<(), ErrorKind> {
    vec.try_reserve(1).map_err(|_| ErrorKind::OutOfMemory)?;
    vec.spare_capacity_mut()
        .first_mut()
        .ok_or(ErrorKind::OutOfMemory)?
        .write(item);
    unsafe { vec.set_len(vec.len() + 1) };
    Ok(())
}

/// `len` zeroed bytes appended to `vec`, which reserves exactly them.
///
/// # Errors
/// [`ErrorKind::OutOfMemory`] when they cannot be reserved.
pub fn zeroed<A: alloc::alloc::Allocator>(
    vec: &mut Vec<u8, A>,
    len: usize,
) -> Result<&mut [u8], ErrorKind> {
    vec.try_reserve_exact(len)
        .map_err(|_| ErrorKind::OutOfMemory)?;
    vec.spare_capacity_mut()
        .iter_mut()
        .take(len)
        .for_each(|byte| {
            byte.write(0);
        });
    let start = vec.len();
    unsafe { vec.set_len(start + len) };
    vec.get_mut(start..).ok_or(ErrorKind::Unsupported)
}

/// [`at`], with no slot past element `N`, which it refuses before reserving.
///
/// # Safety
/// As [`at`].
pub unsafe fn at_limited<T, const N: usize>(vec: *mut u8, len: usize) -> *mut u8 {
    if len < N {
        unsafe { at::<T>(vec, len) }
    } else {
        unsafe { (*vec.cast::<Vec<T>>()).set_len(len) };
        ptr::null_mut()
    }
}

/// # Safety
/// `vec` points to a valid `Vec<T>`.
pub unsafe fn view<T>(vec: *const u8) -> (*const u8, usize) {
    let vec = unsafe { &*vec.cast::<Vec<T>>() };
    (vec.as_ptr().cast(), vec.len())
}

/// Copies the next segment of a byte string into its vector.
///
/// # Safety
/// `dst` points to a valid `Vec<u8>`.
pub unsafe fn append_bytes(dst: *mut u8, _: usize, segment: &[u8]) -> Result<(), ErrorKind> {
    extend(unsafe { &mut *dst.cast::<Vec<u8>>() }, segment)
}

/// Copies the next segment of text.
///
/// # Safety
/// `dst` points to a valid `String`.
pub unsafe fn append_text(dst: *mut u8, _: usize, segment: &[u8]) -> Result<(), ErrorKind> {
    extend(
        unsafe { (*dst.cast::<alloc::string::String>()).as_mut_vec() },
        segment,
    )
}

/// [`append_bytes`], which keeps the byte string within `N` bytes.
///
/// # Safety
/// As [`append_bytes`].
pub unsafe fn append_bytes_limited<const N: usize>(
    dst: *mut u8,
    at: usize,
    segment: &[u8],
) -> Result<(), ErrorKind> {
    within::<N>(at, segment)?;
    unsafe { append_bytes(dst, at, segment) }
}

/// [`append_text`], which keeps the text within `N` bytes.
///
/// # Safety
/// As [`append_text`].
pub unsafe fn append_text_limited<const N: usize>(
    dst: *mut u8,
    at: usize,
    segment: &[u8],
) -> Result<(), ErrorKind> {
    within::<N>(at, segment)?;
    unsafe { append_text(dst, at, segment) }
}

fn extend(vec: &mut Vec<u8>, segment: &[u8]) -> Result<(), ErrorKind> {
    vec.try_reserve(segment.len())
        .map_err(|_| ErrorKind::OutOfMemory)?;
    vec.spare_capacity_mut()
        .iter_mut()
        .zip(segment)
        .for_each(|(slot, byte)| {
            slot.write(*byte);
        });
    unsafe { vec.set_len(vec.len() + segment.len()) };
    Ok(())
}

/// Makes room for a `T`, which it writes into a `Box<T>`'s slot and returns; null when refused.
pub type AllocateFn = unsafe fn(*mut u8) -> *mut u8;

/// An [`AllocateFn`] for `T`. A zero-sized `T` takes no allocation.
///
/// # Safety
/// `slot` is valid for writes of a `Box<T>`.
pub unsafe fn allocate<T>(slot: *mut u8) -> *mut u8 {
    use core::ptr::NonNull;

    let room = if size_of::<T>() == 0 {
        NonNull::<T>::dangling().as_ptr().cast()
    } else {
        unsafe { alloc::alloc::alloc(Layout::new::<T>()) }
    };
    if !room.is_null() {
        unsafe { slot.cast::<*mut u8>().write(room) };
    }
    room
}

/// Frees the room [`allocate`] wrote into `slot`, without dropping anything in it.
///
/// # Safety
/// `slot` holds what [`allocate`] for `T` wrote, and nothing there still needs dropping.
pub unsafe fn release<T>(slot: *mut u8) {
    if size_of::<T>() != 0 {
        unsafe { alloc::alloc::dealloc(slot.cast::<*mut u8>().read(), Layout::new::<T>()) };
    }
}

/// The value a `Box<T>` at `boxed` holds.
///
/// # Safety
/// `boxed` points to a valid `Box<T>`.
pub unsafe fn unboxed<T>(boxed: *const u8) -> *const u8 {
    ptr::from_ref::<T>(unsafe { &*boxed.cast::<alloc::boxed::Box<T>>() }).cast()
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::zeroed;

    #[test]
    fn zeroed_appends_and_returns_only_the_new_bytes() {
        let mut vec = Vec::from([1u8, 2]);
        assert_eq!(zeroed(&mut vec, 3).map(|new| new.len()), Ok(3));
        assert_eq!(vec, [1, 2, 0, 0, 0]);
    }
}
