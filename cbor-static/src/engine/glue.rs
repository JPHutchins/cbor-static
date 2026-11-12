use core::mem::needs_drop;

use crate::schema::{Assemble, EnumGlue};

pub type IndexFn = unsafe fn(*const u8) -> u8;
pub type FieldPtrFn = unsafe fn(*const u8, u8) -> *const u8;
/// The payload of an `Option`, or null for `None`.
pub type SomeFn = unsafe fn(*const u8) -> *const u8;
pub type WriteFn = unsafe fn(*mut u8);
/// Sets a sequence's length to `len`, committing the elements before it, and returns the slot of
/// element `len`, or null at capacity.
pub type AtFn = unsafe fn(*mut u8, usize) -> *mut u8;
/// A sequence's first element and its length.
pub type ViewFn = unsafe fn(*const u8) -> (*const u8, usize);
/// Whether the key of a map's entry `len`, decoded but not committed, differs from every earlier
/// one.
pub type FreshFn = unsafe fn(*const u8, usize) -> bool;
pub type LenFn = unsafe fn(*const u8) -> usize;
/// A map's entry `index`: its key and value, or nulls past the end.
pub type EntryFn = unsafe fn(*const u8, usize) -> (*const u8, *const u8);

/// # Safety
/// `e` points to a valid `E`.
#[must_use]
pub unsafe fn index_of<E: EnumGlue>(e: *const u8) -> u8 {
    unsafe { &*e.cast::<E>() }.variant_index()
}

/// # Safety
/// `e` points to a valid `E`.
#[must_use]
pub unsafe fn field_ptr_of<E: EnumGlue>(e: *const u8, field: u8) -> *const u8 {
    unsafe { &*e.cast::<E>() }.field_ptr(field)
}

/// # Safety
/// `e` points to a valid `#[repr(u8)]` or `#[repr(C, u8)]` enum with implicit discriminants.
#[must_use]
pub unsafe fn index_tag(e: *const u8) -> u8 {
    unsafe { e.read() }
}

/// # Safety
/// `slot` holds an initialized `E::Shadow` and has `E`'s size and alignment.
pub unsafe fn assemble<E: Assemble<V>, const V: u8>(slot: *mut u8) {
    unsafe {
        let shadow = slot.cast::<E::Shadow>().read();
        slot.cast::<E>().write(E::assemble(shadow));
    }
}

/// # Safety
/// `slot` is a primitive-`repr` enum whose variant `V` payload is initialized.
pub unsafe fn write_tag<const V: u8>(slot: *mut u8) {
    unsafe { slot.write(V) };
}

/// # Safety
/// `slot` holds an initialized `T` and has `Option<T>`'s size and alignment.
pub unsafe fn some<T>(slot: *mut u8) {
    unsafe {
        slot.cast::<Option<T>>()
            .write(Some(slot.cast::<T>().read()));
    }
}

/// # Safety
/// `option` points to a valid `Option<T>`.
#[must_use]
pub unsafe fn some_ptr<T>(option: *const u8) -> *const u8 {
    unsafe { &*option.cast::<Option<T>>() }
        .as_ref()
        .map_or(core::ptr::null(), |value| core::ptr::from_ref(value).cast())
}

unsafe fn drop_erased<T>(value: *mut u8) {
    unsafe { value.cast::<T>().drop_in_place() };
}

#[must_use]
pub const fn drop_fn<T>() -> Option<WriteFn> {
    if needs_drop::<T>() {
        Some(drop_erased::<T>)
    } else {
        None
    }
}

/// Whether `S` holds exactly a `T` at `offset`: what lets a unit or newtype struct reuse `T`'s
/// descriptor.
#[must_use]
pub const fn wraps<S, T>(offset: usize) -> bool {
    offset == 0 && size_of::<S>() == size_of::<T>() && align_of::<S>() == align_of::<T>()
}

/// # Panics
/// If `bytes` exceeds `u32::MAX`.
#[must_use]
#[expect(clippy::cast_possible_truncation, reason = "asserted first")]
pub const fn offset(bytes: usize) -> u32 {
    assert!(bytes <= u32::MAX as usize, "field offset exceeds u32");
    bytes as u32
}
