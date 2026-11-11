use core::mem::needs_drop;

use crate::schema::{Assemble, EnumGlue, WriteFn};

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

/// # Panics
/// If `bytes` exceeds `u32::MAX`.
#[must_use]
#[expect(clippy::cast_possible_truncation, reason = "asserted first")]
pub const fn offset(bytes: usize) -> u32 {
    assert!(bytes <= u32::MAX as usize, "field offset exceeds u32");
    bytes as u32
}
