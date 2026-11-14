//! Const access to a [`Storage`]: the arrays a flattening struct's layout and kinds live in, whose
//! lengths only the type system knows when a flattened field's type is generic (R-127). Every
//! `Storage<X>` is laid out as `[X; LEN]`, which the sealed impls guarantee, so each access here is
//! an element of that array.

use core::mem::MaybeUninit;

use crate::schema::Storage;

/// Storage of `S::LEN` copies of `x`.
#[must_use]
pub const fn filled<X: Copy, S: Storage<X>>(x: X) -> S {
    let mut out = MaybeUninit::<S>::uninit();
    let mut at = 0;
    while at < len::<X, S>() {
        unsafe { out.as_mut_ptr().cast::<X>().add(at).write(x) };
        at += 1;
    }
    unsafe { out.assume_init() }
}

/// Storage holding `runs`, one after another.
///
/// # Panics
/// At compile time, when the runs do not fill `S` exactly.
#[must_use]
pub const fn gathered<X: Copy, S: Storage<X>>(runs: &[&[X]]) -> S {
    let mut out = MaybeUninit::<S>::uninit();
    let (mut run, mut at) = (0, 0);
    while run < runs.len() {
        let mut each = 0;
        while each < runs[run].len() {
            assert!(
                at < len::<X, S>(),
                "cbor-static: more entries than their storage"
            );
            unsafe { out.as_mut_ptr().cast::<X>().add(at).write(runs[run][each]) };
            (each, at) = (each + 1, at + 1);
        }
        run += 1;
    }
    assert!(
        at == len::<X, S>(),
        "cbor-static: fewer entries than their storage"
    );
    unsafe { out.assume_init() }
}

/// Element `at` of `storage`.
///
/// # Panics
/// At compile time, past the end.
#[must_use]
pub const fn get<X: Copy, S: Storage<X>>(storage: &S, at: usize) -> X {
    as_slice(storage)[at]
}

/// Sets element `at` of `storage` to `x`.
///
/// # Panics
/// At compile time, past the end.
pub const fn set<X: Copy, S: Storage<X>>(storage: &mut S, at: usize, x: X) {
    assert!(at < len::<X, S>(), "cbor-static: an entry past its storage");
    unsafe { core::ptr::from_mut(storage).cast::<X>().add(at).write(x) };
}

/// `storage`'s elements.
#[must_use]
pub const fn as_slice<X, S: Storage<X>>(storage: &S) -> &[X] {
    unsafe { core::slice::from_raw_parts(core::ptr::from_ref(storage).cast::<X>(), len::<X, S>()) }
}

/// `S`'s length, which the sealed impls lay out as `[X; LEN]`: asserted, since every access here
/// relies on it.
const fn len<X, S: Storage<X>>() -> usize {
    assert!(
        size_of::<S>() == S::LEN * size_of::<X>()
            && (S::LEN == 0 || align_of::<S>() == align_of::<X>()),
        "cbor-static: a storage is not laid out as its array"
    );
    S::LEN
}

#[cfg(test)]
mod tests {
    use super::{as_slice, filled, gathered, get, set};
    use crate::schema::Concat;

    type Five = Concat<[u16; 2], Concat<[u16; 0], [u16; 3]>>;

    #[test]
    fn a_storage_of_storages_reads_as_one_array() {
        let mut five: Five = gathered(&[&[1, 2], &[], &[3, 4, 5]]);
        assert_eq!(as_slice(&five), [1, 2, 3, 4, 5]);
        set(&mut five, 3, 9);
        assert_eq!((get(&five, 3), get(&five, 4)), (9, 5));
        assert_eq!(as_slice(&filled::<u16, Five>(7)), [7; 5]);
    }

    #[test]
    #[should_panic(expected = "fewer entries than their storage")]
    fn the_runs_must_fill_their_storage() {
        let _: Five = gathered(&[&[1, 2]]);
    }
}
