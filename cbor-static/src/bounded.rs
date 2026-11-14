#![deny(unsafe_code)]
//! Fixed-capacity containers from heapless 0.9, behind feature `heapless` (#12 Q19, Q21).

use core::mem::offset_of;

use heapless::{LenType, LinearMap, String, Vec};

use crate::engine::{
    BYTES_OWNED, DECODE_LINEAR_MAP, DECODE_SEQ, ENCODE_LINEAR_MAP, ENCODE_SEQ, TEXT_OWNED,
    any_length, append_string, append_vec, assemble_map, at, drop_fn, empty, enc_string, enc_view,
    entry, fresh, len_of, shadow_fits, view,
};
use crate::lead::Lead;
use crate::schema::{ByteString, DecKind, Decode, EncKind, Encode, MapGlue, Order, sealed};
use crate::stack::{Level, Twice, Widest, deeper};

#[allow(unsafe_code)]
unsafe impl<T: Encode, const N: usize, L: LenType> Encode for Vec<T, N, L> {
    const ENCODE: &'static EncKind = &EncKind::Seq {
        ops: &ENCODE_SEQ,
        elem: T::ENCODE,
        stride: size_of::<T>(),
        view: view::<T, N, L>,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    const ORDER: Order = T::ORDER;
}

/// An array of up to `N` elements, each decoded in place; without `#[cbor(bytes)]`, a `Vec<u8>` is
/// one too, as in serde.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let values: &mut heapless::Vec<u16, 4, u8> = cbor_static::decode_into(b"\x9f\x01\x19\x01\x00\xff", &mut slot)?;
/// assert_eq!(values, &[1, 256]);
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::encode_into(&*values, &mut out)?;
/// assert_eq!(&out[..len], b"\x82\x01\x19\x01\x00");
///
/// let mut slot = core::mem::MaybeUninit::<heapless::Vec<u16, 1>>::uninit();
/// assert_eq!(
///     cbor_static::decode_into(b"\x82\x01\x02", &mut slot).map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::OutOfRange)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>, const N: usize, L: LenType> Decode<'de> for Vec<T, N, L> {
    const DECODE: &'static DecKind = &DecKind::Seq {
        ops: &DECODE_SEQ,
        begin: empty::<Self>,
        drop: drop_fn::<Self>(),
        elem: T::DECODE,
        at: at::<T, N, L>,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::ARRAY;
}

/// A map of its entries in their order; under a deterministic root (D-11b), in their keys' order,
/// found as each entry is written by encoding the keys on room the map's stack keeps for two of
/// `K`'s encoders, which is none for a scalar or string key.
///
/// ```
/// use heapless::LinearMap;
///
/// #[derive(cbor_static::Encode)]
/// #[cbor(deterministic)]
/// struct Sorted(LinearMap<u16, bool, 4>);
///
/// let map: LinearMap<u16, bool, 4> = [(1000, true), (5, false)].into_iter().collect();
/// let mut out = [0u8; 8];
/// let len = cbor_static::encode_into(&map, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x19\x03\xe8\xf5\x05\xf4");
/// let len = cbor_static::encode_into(&Sorted(map), &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x05\xf4\x19\x03\xe8\xf5");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<K: Encode + Eq, V: Encode, const N: usize> Encode for LinearMap<K, V, N> {
    const ENCODE: &'static EncKind = &EncKind::Map {
        ops: &ENCODE_LINEAR_MAP,
        key: K::ENCODE,
        value: V::ENCODE,
        len: len_of::<K, V, N>,
        entry: entry::<K, V, N>,
    };
    type Stack = Level<Widest<Widest<K::Stack, V::Stack>, Twice<K::Stack>>>;
    const MAX_DEPTH: usize = 1 + deeper(deeper(K::MAX_DEPTH, V::MAX_DEPTH), 2 * K::MAX_DEPTH);
    const ORDER: Order = K::ORDER.join(V::ORDER);
}

/// A map of up to `N` entries, in input order, whose keys differ (D-12). Under `Strict`, each key's
/// encoding must sort after the one before it (Q29): comparing them encodes both keys, on room the
/// map's own stack keeps for two of `K`'s encoders, which is none for a scalar or string key.
///
/// ```
/// use heapless::LinearMap;
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let map: &mut LinearMap<u8, bool, 4> = cbor_static::decode_into(b"\xa2\x02\xf5\x01\xf4", &mut slot)?;
/// assert_eq!(map.iter().collect::<heapless::Vec<_, 4>>(), [(&2, &true), (&1, &false)]);
///
/// let mut slot = core::mem::MaybeUninit::<LinearMap<u8, bool, 4>>::uninit();
/// assert_eq!(
///     cbor_static::decode_into(b"\xa2\x01\xf5\x01\xf4", &mut slot).map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::DuplicateKey)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<'de, K: Decode<'de> + Encode + Eq, V: Decode<'de>, const N: usize> Decode<'de>
    for LinearMap<K, V, N>
{
    const DECODE: &'static DecKind = &{
        assert!(
            shadow_fits::<K, V, N>(),
            "cbor-static: a LinearMap's slot cannot hold its entries"
        );
        DecKind::Map {
            ops: &DECODE_LINEAR_MAP,
            key: <K as Decode<'de>>::DECODE,
            drop: drop_fn::<Self>(),
            value: V::DECODE,
            glue: &MapGlue {
                begin: empty::<Vec<(K, V), N>>,
                at: at::<(K, V), N, usize>,
                key_at: offset_of!((K, V), 0),
                value_at: offset_of!((K, V), 1),
                stride: size_of::<(K, V)>(),
                fresh: fresh::<K, V, N>,
                drop_key: drop_fn::<K>(),
                drop_shadow: drop_fn::<Vec<(K, V), N>>(),
                assemble: assemble_map::<K, V, N>,
                key_encode: <K as Encode>::ENCODE,
            },
        }
    };
    type Stack =
        Level<Widest<Widest<<K as Decode<'de>>::Stack, V::Stack>, Twice<<K as Encode>::Stack>>>;
    const MAX_DEPTH: usize = 1 + deeper(
        deeper(<K as Decode<'de>>::MAX_DEPTH, V::MAX_DEPTH),
        2 * <K as Encode>::MAX_DEPTH,
    );
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::MAP;
}

#[allow(unsafe_code)]
unsafe impl<const N: usize, L: LenType> Encode for String<N, L> {
    const ENCODE: &'static EncKind = &EncKind::Leaf {
        encode: enc_string::<N, L>,
        absent: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

/// Any text string of at most `N` bytes, definite or in chunks, which may arrive in chunks of
/// input too (D-13, D-16).
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let name: &mut heapless::String<4> = cbor_static::decode_into(b"\x7f\x62ab\x61c\xff", &mut slot)?;
/// assert_eq!(name.as_str(), "abc");
///
/// let mut slot = core::mem::MaybeUninit::<heapless::String<2>>::uninit();
/// assert_eq!(
///     cbor_static::decode_into(b"\x63abc", &mut slot).map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::OutOfRange)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<const N: usize, L: LenType> Decode<'_> for String<N, L> {
    const DECODE: &'static DecKind = &DecKind::Owned {
        ops: &TEXT_OWNED,
        begin: empty::<Self>,
        drop: drop_fn::<Self>(),
        append: append_string::<N, L>,
        finish: any_length,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::TEXT;
}

/// `#[cbor(bytes)]`: any byte string of at most `N` bytes; without it, an array.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Frame {
///     #[cbor(bytes)]
///     payload: heapless::Vec<u8, 8>,
/// }
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let frame: &mut Frame = cbor_static::decode_into(b"\xa1\x67payload\x5f\x41\x01\x42\x02\x03\xff", &mut slot)?;
/// assert_eq!(frame.payload, [1, 2, 3]);
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::encode_into(&*frame, &mut out)?;
/// assert_eq!(&out[..len], b"\xa1\x67payload\x43\x01\x02\x03");
/// # Ok::<(), cbor_static::Error>(())
/// ```
impl<const N: usize, L: LenType> sealed::ByteString for Vec<u8, N, L> {}

impl<const N: usize, L: LenType> ByteString for Vec<u8, N, L> {
    const ENCODE_BYTES: &'static EncKind = &EncKind::Leaf {
        encode: enc_view::<Self>,
        absent: None,
    };
    const DECODE_BYTES: &'static DecKind = &DecKind::Owned {
        ops: &BYTES_OWNED,
        begin: empty::<Self>,
        drop: drop_fn::<Self>(),
        append: append_vec::<N, L>,
        finish: any_length,
    };
}
