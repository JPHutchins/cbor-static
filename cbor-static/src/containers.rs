#![deny(unsafe_code)]
//! Tuples, as arrays of their elements (D-20).

use core::mem::offset_of;

use crate::engine::{DECODE_ARRAY, ENCODE_ARRAY, drop_fn, offset};
use crate::schema::{DecKind, Decode, EncKind, Encode, FieldLayout, Layout};
use crate::stack::{Level, Widest, deeper};

/// A tuple element `at` bytes in: positional, so it has no name and is never omitted.
const fn element(at: usize) -> FieldLayout {
    FieldLayout {
        offset: offset(at),
        name_at: 0,
        name_len: 0,
        omit_if_absent: false,
    }
}

macro_rules! tuple {
    ($($T:ident $index:tt),+) => {
        #[allow(unsafe_code)]
        unsafe impl<$($T: Encode),+> Encode for ($($T,)+) {
            const ENCODE: &'static EncKind = &EncKind::Struct {
                ops: &ENCODE_ARRAY,
                layout: &Layout {
                    names: &[],
                    fields: &[$(element(offset_of!(Self, $index))),+],
                },
                kinds: &[$($T::ENCODE),+],
                field_ptr: None,
            };
            type Stack = Level<tuple!(@widest $($T::Stack),+)>;
            const MAX_DEPTH: usize = 1 + tuple!(@deepest $($T::MAX_DEPTH),+);
        }

        #[allow(unsafe_code)]
        unsafe impl<'de, $($T: Decode<'de>),+> Decode<'de> for ($($T,)+) {
            const DECODE: &'static DecKind = &DecKind::Struct {
                ops: &DECODE_ARRAY,
                layout: &Layout {
                    names: &[],
                    fields: &[$(element(offset_of!(Self, $index))),+],
                },
                drop: drop_fn::<Self>(),
                kinds: &[$($T::DECODE),+],
            };
            type Stack = Level<tuple!(@widest $($T::Stack),+)>;
            const MAX_DEPTH: usize = 1 + tuple!(@deepest $($T::MAX_DEPTH),+);
        }
    };
    (@widest $first:ty $(, $rest:ty)*) => { Widest<$first, tuple!(@widest $($rest),*)> };
    (@widest) => { () };
    (@deepest $first:expr $(, $rest:expr)*) => { deeper($first, tuple!(@deepest $($rest),*)) };
    (@deepest) => { 0 };
}

tuple!(A 0);
tuple!(A 0, B 1);
tuple!(A 0, B 1, C 2);
tuple!(A 0, B 1, C 2, D 3);
