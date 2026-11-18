#![deny(unsafe_code)]
//! `#[cbor(max_len = N)]` (#5 Q15): the types whose `len()` it bounds, and the kinds a field of
//! one takes under it, which a type without such fields never names.

use core::marker::PhantomData;

use crate::engine::{DECODE_OPTION, drop_fn, limited, none, some};
use crate::schema::{DecKind, Decode, EncKind};
use crate::sizes::{MaxEncodedLen, container_len, larger};

/// A type whose `len()` a field's `max_len` bounds: bytes for strings, elements for sequences,
/// entries for maps.
#[diagnostic::on_unimplemented(
    message = "`max_len` does not bound `{Self}`",
    note = "it bounds `&str`, `&[u8]`, alloc or heapless `String` and `Vec`, heapless `LinearMap`, and an `Option` of one"
)]
pub trait Limit {
    /// The longest `len()` the type itself allows.
    const BUILT_IN: usize;
    fn length(&self) -> usize;
}

/// The kinds and bound of a `T` field under `max_len = N`.
pub struct Limited<T, const N: usize>(PhantomData<T>);

/// A `Limited` field's kind for decoding.
#[diagnostic::on_unimplemented(message = "`max_len` does not bound this field's type")]
pub trait LimitedDecode<'de> {
    const DECODE: &'static DecKind;
}

/// `inner`, a `T` field's own kind or its byte string's, behind the check of its length against
/// `max_len = N`.
#[must_use]
pub const fn encode_limited<T: Limit, const N: usize>(inner: &'static EncKind) -> EncKind {
    allowed::<T, N>();
    EncKind::limited(inner, limited::<T, N>)
}

/// `len()` itself, under the type's own limit.
macro_rules! limit {
    ($([$($generics:tt)*] $ty:ty => $built_in:expr),+ $(,)?) => {$(
        impl<$($generics)*> $crate::limits::Limit for $ty {
            const BUILT_IN: usize = $built_in;
            fn length(&self) -> usize {
                self.len()
            }
        }
    )+};
}

limit!([] &str => usize::MAX, [] &[u8] => usize::MAX);

/// A `None` is no longer than any limit.
impl<T: Limit> Limit for Option<T> {
    const BUILT_IN: usize = T::BUILT_IN;
    fn length(&self) -> usize {
        self.as_ref().map_or(0, T::length)
    }
}

impl<'de: 'a, 'a, const N: usize> LimitedDecode<'de> for Limited<&'a str, N> {
    const DECODE: &'static DecKind = &DecKind::Leaf {
        decode: crate::engine::str_limited::<N>(),
        default: None,
        drop: None,
    };
}

impl<'de: 'a, 'a, const N: usize> LimitedDecode<'de> for Limited<&'a [u8], N> {
    const DECODE: &'static DecKind = &DecKind::Leaf {
        decode: crate::engine::bytes_limited::<N>(),
        default: None,
        drop: None,
    };
}

/// `T`'s `Option`, as [`Decode::OPTION_DECODE`] gives it, around the limited `T`.
impl<'de, T: Decode<'de>, const N: usize> LimitedDecode<'de> for Limited<Option<T>, N>
where
    Limited<T, N>: LimitedDecode<'de>,
{
    const DECODE: &'static DecKind = &DecKind::Option {
        ops: &DECODE_OPTION,
        default: Some(none::<T>),
        drop: drop_fn::<Option<T>>(),
        some: <Limited<T, N> as LimitedDecode<'de>>::DECODE,
        assemble: some::<T>,
    };
}

impl<const N: usize> MaxEncodedLen for Limited<&str, N> {
    const MAX_ENCODED_LEN: usize = container_len(N, 1);
}

impl<const N: usize> MaxEncodedLen for Limited<&[u8], N> {
    const MAX_ENCODED_LEN: usize = container_len(N, 1);
}

impl<T, const N: usize> MaxEncodedLen for Limited<Option<T>, N>
where
    Limited<T, N>: MaxEncodedLen,
{
    const MAX_ENCODED_LEN: usize = larger(
        <() as MaxEncodedLen>::MAX_ENCODED_LEN,
        <Limited<T, N> as MaxEncodedLen>::MAX_ENCODED_LEN,
    );
}

/// The decoding of `#[cbor(bytes)]` under `max_len = N`.
pub trait LimitedBytes {
    const DECODE_BYTES: &'static DecKind;
}

const fn allowed<T: Limit, const N: usize>() {
    assert!(
        N <= T::BUILT_IN,
        "cbor-static: `max_len` is above the field type's own limit"
    );
}

#[cfg(feature = "heapless")]
mod bounded {
    use heapless::{LenType, LinearMap, String, Vec};

    use super::{Limited, LimitedBytes, LimitedDecode, allowed};
    use crate::bounded::glue;
    use crate::engine::{
        DecodeLimited, any_length, append_string_limited, append_vec_limited, at_limited, drop_fn,
        empty,
    };
    use crate::schema::{DecKind, Decode, Encode};
    use crate::sizes::{MaxEncodedLen, container_len};

    limit!(
        [const CAP: usize, L: LenType] String<CAP, L> => CAP,
        [T, const CAP: usize, L: LenType] Vec<T, CAP, L> => CAP,
        [K: Eq, V, const CAP: usize] LinearMap<K, V, CAP> => CAP,
    );

    impl<const CAP: usize, L: LenType, const N: usize> LimitedDecode<'_>
        for Limited<String<CAP, L>, N>
    {
        const DECODE: &'static DecKind = &{
            allowed::<String<CAP, L>, N>();
            DecKind::Owned {
                ops: &DecodeLimited::<N>::TEXT,
                begin: empty::<String<CAP, L>>,
                drop: drop_fn::<String<CAP, L>>(),
                append: append_string_limited::<CAP, L, N>,
                finish: any_length,
            }
        };
    }

    impl<'de, T: Decode<'de>, const CAP: usize, L: LenType, const N: usize> LimitedDecode<'de>
        for Limited<Vec<T, CAP, L>, N>
    {
        const DECODE: &'static DecKind = &{
            allowed::<Vec<T, CAP, L>, N>();
            DecKind::Seq {
                ops: &DecodeLimited::<N>::SEQ,
                begin: empty::<Vec<T, CAP, L>>,
                drop: drop_fn::<Vec<T, CAP, L>>(),
                elem: T::DECODE,
                at: at_limited::<T, CAP, L, N>,
            }
        };
    }

    impl<'de, K, V, const CAP: usize, const N: usize> LimitedDecode<'de>
        for Limited<LinearMap<K, V, CAP>, N>
    where
        LinearMap<K, V, CAP>: Decode<'de>,
        K: Decode<'de> + Encode + Eq,
        V: Decode<'de>,
    {
        const DECODE: &'static DecKind = &{
            allowed::<LinearMap<K, V, CAP>, N>();
            DecKind::Map {
                ops: &DecodeLimited::<N>::LINEAR_MAP,
                key: <K as Decode<'de>>::DECODE,
                drop: drop_fn::<LinearMap<K, V, CAP>>(),
                value: V::DECODE,
                glue: &glue::<K, V, CAP>(at_limited::<(K, V), CAP, usize, N>),
            }
        };
    }

    impl<const CAP: usize, L: LenType, const N: usize> LimitedBytes for Limited<Vec<u8, CAP, L>, N> {
        const DECODE_BYTES: &'static DecKind = &{
            allowed::<Vec<u8, CAP, L>, N>();
            DecKind::Owned {
                ops: &DecodeLimited::<N>::BYTES,
                begin: empty::<Vec<u8, CAP, L>>,
                drop: drop_fn::<Vec<u8, CAP, L>>(),
                append: append_vec_limited::<CAP, L, N>,
                finish: any_length,
            }
        };
    }

    impl<const CAP: usize, L: LenType, const N: usize> MaxEncodedLen for Limited<String<CAP, L>, N> {
        const MAX_ENCODED_LEN: usize = container_len(N, 1);
    }

    impl<T: MaxEncodedLen, const CAP: usize, L: LenType, const N: usize> MaxEncodedLen
        for Limited<Vec<T, CAP, L>, N>
    {
        const MAX_ENCODED_LEN: usize = container_len(N, T::MAX_ENCODED_LEN);
    }

    impl<K: MaxEncodedLen, V: MaxEncodedLen, const CAP: usize, const N: usize> MaxEncodedLen
        for Limited<LinearMap<K, V, CAP>, N>
    {
        const MAX_ENCODED_LEN: usize = container_len(N, K::MAX_ENCODED_LEN + V::MAX_ENCODED_LEN);
    }
}

#[cfg(feature = "alloc")]
mod allocated {
    use alloc::string::String;
    use alloc::vec::Vec;

    use super::{Limited, LimitedBytes, LimitedDecode};
    use crate::engine::{
        HeapLimited, any_length, append_bytes_limited, append_text_limited, at_heap_limited,
        drop_fn, empty,
    };
    use crate::schema::{DecKind, Decode};
    use crate::sizes::{MaxEncodedLen, container_len};

    limit!([] String => usize::MAX, [T] Vec<T> => usize::MAX);

    impl<const N: usize> LimitedDecode<'_> for Limited<String, N> {
        const DECODE: &'static DecKind = &DecKind::Owned {
            ops: &HeapLimited::<N>::TEXT,
            begin: empty::<String>,
            drop: drop_fn::<String>(),
            append: append_text_limited::<N>,
            finish: any_length,
        };
    }

    impl<'de, T: Decode<'de>, const N: usize> LimitedDecode<'de> for Limited<Vec<T>, N> {
        const DECODE: &'static DecKind = &DecKind::Seq {
            ops: &HeapLimited::<N>::SEQ,
            begin: empty::<Vec<T>>,
            drop: drop_fn::<Vec<T>>(),
            elem: T::DECODE,
            at: at_heap_limited::<T, N>,
        };
    }

    impl<const N: usize> LimitedBytes for Limited<Vec<u8>, N> {
        const DECODE_BYTES: &'static DecKind = &DecKind::Owned {
            ops: &HeapLimited::<N>::BYTES,
            begin: empty::<Vec<u8>>,
            drop: drop_fn::<Vec<u8>>(),
            append: append_bytes_limited::<N>,
            finish: any_length,
        };
    }

    impl<const N: usize> MaxEncodedLen for Limited<String, N> {
        const MAX_ENCODED_LEN: usize = container_len(N, 1);
    }

    impl<T: MaxEncodedLen, const N: usize> MaxEncodedLen for Limited<Vec<T>, N> {
        const MAX_ENCODED_LEN: usize = container_len(N, T::MAX_ENCODED_LEN);
    }
}
