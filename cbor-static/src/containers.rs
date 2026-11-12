#![deny(unsafe_code)]
//! Tuples and `[T; N]`, as arrays of their elements (D-20).

use core::mem::{offset_of, size_of};

use crate::engine::{
    BYTES_OWNED, DECODE_ARRAY, DECODE_ELEMENTS, ENCODE_ARRAY, ENCODE_ELEMENTS, drop_fn, enc_view,
    exactly, nothing, offset, within,
};
use crate::lead::Lead;
use crate::schema::{ByteString, DecKind, Decode, EncKind, Encode, FieldLayout, Layout, sealed};
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
            type OptionStack = Level<Self::Stack>;
            const LEAD: Lead = Lead::ARRAY;
        }
    };
    (@widest $first:ty $(, $rest:ty)*) => { Widest<$first, tuple!(@widest $($rest),*)> };
    (@widest) => { () };
    (@deepest $first:expr $(, $rest:expr)*) => { deeper($first, tuple!(@deepest $($rest),*)) };
    (@deepest) => { 0 };
}

#[allow(unsafe_code)]
unsafe impl<T: Encode, const N: usize> Encode for [T; N] {
    const ENCODE: &'static EncKind = &EncKind::Array {
        ops: &ENCODE_ELEMENTS,
        elem: T::ENCODE,
        len: N,
        stride: size_of::<T>(),
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
}

#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>, const N: usize> Decode<'de> for [T; N] {
    const DECODE: &'static DecKind = &DecKind::Array {
        ops: &DECODE_ELEMENTS,
        elem: T::DECODE,
        drop: drop_fn::<Self>(),
        len: N,
        stride: size_of::<T>(),
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::ARRAY;
}

impl<const N: usize> sealed::ByteString for [u8; N] {}

/// `#[cbor(bytes)]`: a byte string of exactly `N` bytes, which may arrive in chunks.
impl<const N: usize> ByteString for [u8; N] {
    const ENCODE_BYTES: &'static EncKind = &EncKind::Leaf {
        encode: enc_view::<Self>,
        absent: None,
    };
    const DECODE_BYTES: &'static DecKind = &DecKind::Owned {
        ops: &BYTES_OWNED,
        begin: nothing,
        drop: None,
        append: within::<N>,
        finish: exactly::<N>,
    };
}

tuple!(A 0);
tuple!(A 0, B 1);
tuple!(A 0, B 1, C 2);
tuple!(A 0, B 1, C 2, D 3);
