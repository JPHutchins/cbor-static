//! The per-type side of heapless sequences and maps: where the engine decodes each element in
//! place, how long the container is, and how a `LinearMap` is built from the shadow `Vec` its
//! entries were decoded into. heapless types have no `repr`, so every access is through their API.
use core::ptr;

use heapless::{LenType, LinearMap, Vec};

pub unsafe fn at<T, const N: usize, L: LenType>(vec: *mut u8, len: usize) -> *mut u8 {
    if len > N {
        return ptr::null_mut();
    }
    let vec = unsafe { &mut *vec.cast::<Vec<T, N, L>>() };
    unsafe { vec.set_len(len) };
    if len < N {
        unsafe { vec.as_mut_ptr().add(len) }.cast()
    } else {
        ptr::null_mut()
    }
}

pub unsafe fn view<T, const N: usize, L: LenType>(vec: *const u8) -> (*const u8, usize) {
    let vec = unsafe { &*vec.cast::<Vec<T, N, L>>() };
    (vec.as_ptr().cast(), vec.len())
}

/// Whether a `LinearMap<K, V, N>`'s slot can hold its shadow `Vec<(K, V), N>`.
#[must_use]
pub const fn shadow_fits<K, V, const N: usize>() -> bool {
    size_of::<Vec<(K, V), N>>() <= size_of::<LinearMap<K, V, N>>()
        && align_of::<Vec<(K, V), N>>() <= align_of::<LinearMap<K, V, N>>()
}

pub unsafe fn fresh<K: Eq, V, const N: usize>(shadow: *const u8, len: usize) -> bool {
    let shadow = unsafe { &*shadow.cast::<Vec<(K, V), N>>() };
    let key = unsafe { &*ptr::addr_of!((*shadow.as_ptr().add(len)).0) };
    !shadow.iter().any(|(earlier, _)| earlier == key)
}

/// Moves the shadow out of the slot once, then fills an empty map in its place: a transient of one
/// map's size (D-15).
pub unsafe fn assemble<K: Eq, V, const N: usize>(slot: *mut u8) {
    let shadow = unsafe { slot.cast::<Vec<(K, V), N>>().read() };
    unsafe { slot.cast::<LinearMap<K, V, N>>().write(LinearMap::new()) };
    let map = unsafe { &mut *slot.cast::<LinearMap<K, V, N>>() };
    shadow.into_iter().for_each(|(key, value)| {
        let _ = map.insert(key, value);
    });
}

pub unsafe fn len_of<K: Eq, V, const N: usize>(map: *const u8) -> usize {
    unsafe { &*map.cast::<LinearMap<K, V, N>>() }.len()
}

pub unsafe fn entry<K: Eq, V, const N: usize>(
    map: *const u8,
    index: usize,
) -> (*const u8, *const u8) {
    unsafe { &*map.cast::<LinearMap<K, V, N>>() }
        .iter()
        .nth(index)
        .map_or((ptr::null(), ptr::null()), |(key, value)| {
            (ptr::from_ref(key).cast(), ptr::from_ref(value).cast())
        })
}
