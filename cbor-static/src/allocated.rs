#![deny(unsafe_code)]
//! Containers that allocate, behind feature `alloc` (#5 Q8).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::offset_of;

use crate::engine::{
    AtFn, BYTES_HEAP, DECODE_BOXED, DECODE_BTREE_MAP, ENCODE_BTREE_MAP, ENCODE_OPTION, ENCODE_SEQ,
    SEQ_HEAP, TEXT_HEAP, after, allocate, any_length, append_bytes, append_text, assemble_btree,
    at_heap, deferred, drop_fn, empty, enc_text, enc_view, entry_btree, len_btree, release, settle,
    shadow_fits_btree, unboxed, view_heap,
};
use crate::lead::Lead;
use crate::schema::{ByteString, DecKind, Decode, EncKind, Encode, MapGlue, Order, sealed};
use crate::sizes::MaxEncodedLen;
use crate::stack::{Level, Twice, Widest, deeper};

#[allow(unsafe_code)]
unsafe impl<T: Encode> Encode for Vec<T> {
    const ENCODE: &'static EncKind = &EncKind::Seq {
        ops: &ENCODE_SEQ,
        elem: T::ENCODE,
        stride: size_of::<T>(),
        view: view_heap::<T>,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    const ORDER: Order = T::ORDER;
}

/// An array of any length, each element decoded in place in a slot reserved as it begins; without
/// `#[cbor(bytes)]`, a `Vec<u8>` is one too, as in serde.
///
/// ```
/// let values: Vec<u16> = cbor_static::from_slice(b"\x9f\x01\x19\x01\x00\xff")?;
/// assert_eq!(values, [1, 256]);
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::to_slice(&values, &mut out)?;
/// assert_eq!(&out[..len], b"\x82\x01\x19\x01\x00");
///
/// let len = cbor_static::to_slice(&vec![1u8, 2], &mut out)?;
/// assert_eq!(&out[..len], b"\x82\x01\x02");
///
/// assert_eq!(
///     cbor_static::from_slice::<Vec<u16>>(b"\x82\x01\xf5").map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::TypeMismatch)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>> Decode<'de> for Vec<T> {
    const DECODE: &'static DecKind = &DecKind::Seq {
        ops: &SEQ_HEAP,
        begin: empty::<Self>,
        drop: drop_fn::<Self>(),
        elem: T::DECODE,
        at: at_heap::<T>,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::ARRAY;
    const CONTIGUOUS: bool = T::CONTIGUOUS;
}

#[allow(unsafe_code)]
unsafe impl Encode for String {
    const ENCODE: &'static EncKind = &EncKind::Leaf {
        encode: enc_text::<Self>,
        absent: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

/// Any text string, definite or in chunks, which may arrive in chunks of input too (D-13, D-16).
///
/// ```
/// let name: String = cbor_static::from_slice(b"\x7f\x62ab\x61c\xff")?;
/// assert_eq!(name, "abc");
///
/// let mut out = [0u8; 4];
/// let len = cbor_static::to_slice(&name, &mut out)?;
/// assert_eq!(&out[..len], b"\x63abc");
///
/// assert_eq!(
///     cbor_static::from_slice::<String>(b"\x62a\xff").map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::InvalidUtf8)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl Decode<'_> for String {
    const DECODE: &'static DecKind = &DecKind::Owned {
        ops: &TEXT_HEAP,
        begin: empty::<Self>,
        drop: drop_fn::<Self>(),
        append: append_text,
        finish: any_length,
    };
    type Stack = Level<()>;
    const MAX_DEPTH: usize = 1;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::TEXT;
    const CONTIGUOUS: bool = Self::DECODE.takes_payload();
}

impl sealed::ByteString for Vec<u8> {}

/// `#[cbor(bytes)]`: any byte string, definite or in chunks; without it, an array.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Frame {
///     #[cbor(bytes)]
///     payload: Vec<u8>,
/// }
///
/// let frame: Frame = cbor_static::from_slice(b"\xa1\x67payload\x5f\x41\x01\x42\x02\x03\xff")?;
/// assert_eq!(frame.payload, [1, 2, 3]);
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&frame, &mut out)?;
/// assert_eq!(&out[..len], b"\xa1\x67payload\x43\x01\x02\x03");
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// Its encoding has no longest length, so a type holding one has no `MaxEncodedLen`:
///
/// ```compile_fail,E0277
/// #[derive(cbor_static::Encode)]
/// #[cbor(max_encoded_len)]
/// struct Frame {
///     #[cbor(bytes)]
///     payload: Vec<u8>,
/// }
/// ```
impl ByteString for Vec<u8> {
    const ENCODE_BYTES: &'static EncKind = &EncKind::Leaf {
        encode: enc_view::<Self>,
        absent: None,
    };
    const DECODE_BYTES: &'static DecKind = &DecKind::Owned {
        ops: &BYTES_HEAP,
        begin: empty::<Self>,
        drop: drop_fn::<Self>(),
        append: append_bytes,
        finish: any_length,
    };
    type Stack = Level<()>;
    const MAX_DEPTH: usize = 1;
}

#[allow(unsafe_code)]
unsafe impl<T: Encode> Encode for Box<T> {
    const ENCODE: &'static EncKind = &EncKind::Option {
        ops: &ENCODE_OPTION,
        absent: None,
        some: T::ENCODE,
        some_ptr: unboxed::<T>,
    };
    type Stack = T::Stack;
    const MAX_DEPTH: usize = T::MAX_DEPTH;
    const ORDER: Order = T::ORDER;
}

/// The value it holds, decoded into room allocated as it begins and freed should it fail.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Reading {
///     id: u8,
///     detail: Option<Box<[u16; 2]>>,
/// }
///
/// let reading: Box<Reading> = cbor_static::from_slice(b"\xa2\x62id\x01\x66detail\x82\x01\x02")?;
/// assert_eq!(reading.detail.as_deref(), Some(&[1, 2]));
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&reading, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x62id\x01\x66detail\x82\x01\x02");
///
/// assert_eq!(
///     cbor_static::from_slice::<Box<Reading>>(b"\xa2\x62id\x01\x66detail\x82\x01\xf5")
///         .map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::TypeMismatch)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<'de, T: Decode<'de>> Decode<'de> for Box<T> {
    const DECODE: &'static DecKind = &DecKind::Boxed {
        ops: &DECODE_BOXED,
        inner: T::DECODE,
        drop: drop_fn::<Self>(),
        allocate: allocate::<T>,
        release: release::<T>,
    };
    type Stack = Level<T::Stack>;
    const MAX_DEPTH: usize = 1 + T::MAX_DEPTH;
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = T::LEAD;
    const CONTIGUOUS: bool = T::CONTIGUOUS;
}

/// The longest encoding of the value it holds.
///
/// ```
/// use cbor_static::MaxEncodedLen;
///
/// assert_eq!(<Box<u32> as MaxEncodedLen>::MAX_ENCODED_LEN, 5);
/// ```
impl<T: MaxEncodedLen> MaxEncodedLen for Box<T> {
    const MAX_ENCODED_LEN: usize = T::MAX_ENCODED_LEN;
}

#[allow(unsafe_code)]
unsafe impl<K: Encode + Ord, V: Encode> Encode for BTreeMap<K, V> {
    const ENCODE: &'static EncKind = &EncKind::Ordered {
        ops: &ENCODE_BTREE_MAP,
        key: K::ENCODE,
        value: V::ENCODE,
        len: len_btree::<K, V>,
        entry: entry_btree::<K, V>,
        after: after::<K, V>,
    };
    type Stack = Level<Widest<Widest<K::Stack, V::Stack>, Twice<K::Stack>>>;
    const MAX_DEPTH: usize = 1 + deeper(deeper(K::MAX_DEPTH, V::MAX_DEPTH), 2 * K::MAX_DEPTH);
    const ORDER: Order = K::ORDER.join(V::ORDER);
}

/// A map whose keys differ (D-12), in any order. Its entries decode into a sequence reserved as
/// each begins, which becomes the map when the map closes: a duplicate key is
/// [`crate::ErrorKind::DuplicateKey`] at the map's end. Under `Strict`, each key's encoding must
/// sort after the one before it (Q29), as it arrives.
///
/// ```
/// use std::collections::BTreeMap;
///
/// let map: BTreeMap<u8, bool> = cbor_static::from_slice(b"\xa2\x02\xf5\x01\xf4")?;
/// assert_eq!(map, BTreeMap::from([(1, false), (2, true)]));
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::to_slice(&map, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x01\xf4\x02\xf5");
///
/// assert_eq!(
///     cbor_static::from_slice::<BTreeMap<u8, bool>>(b"\xa2\x01\xf5\x01\xf4").map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::DuplicateKey)
/// );
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(
///     cbor_static::decode_strict_into::<BTreeMap<u8, bool>>(b"\xa2\x02\xf5\x01\xf4", &mut slot)
///         .map(|_| ())
///         .map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::NotDeterministic)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[allow(unsafe_code)]
unsafe impl<'de, K: Decode<'de> + Encode + Ord, V: Decode<'de>> Decode<'de> for BTreeMap<K, V> {
    const DECODE: &'static DecKind = &DecKind::Map {
        ops: &DECODE_BTREE_MAP,
        key: <K as Decode<'de>>::DECODE,
        drop: drop_fn::<Self>(),
        value: V::DECODE,
        glue: &glue::<K, V>(at_heap::<(K, V)>),
    };
    type Stack =
        Level<Widest<Widest<<K as Decode<'de>>::Stack, V::Stack>, Twice<<K as Encode>::Stack>>>;
    const MAX_DEPTH: usize = 1 + deeper(
        deeper(<K as Decode<'de>>::MAX_DEPTH, V::MAX_DEPTH),
        2 * <K as Encode>::MAX_DEPTH,
    );
    type OptionStack = Level<Self::Stack>;
    const LEAD: Lead = Lead::MAP;
    const CONTIGUOUS: bool = <K as Decode<'de>>::CONTIGUOUS || V::CONTIGUOUS;
}

/// How a `BTreeMap<K, V>`'s entries decode into its shadow, whose slots `at` gives.
pub(crate) const fn glue<K: Encode + Ord, V>(at: AtFn) -> MapGlue {
    assert!(
        shadow_fits_btree::<K, V>(),
        "cbor-static: a BTreeMap's slot cannot hold its entries"
    );
    MapGlue {
        begin: empty::<Vec<(K, V)>>,
        at,
        key_at: offset_of!((K, V), 0),
        value_at: offset_of!((K, V), 1),
        stride: size_of::<(K, V)>(),
        fresh: deferred,
        drop_key: drop_fn::<K>(),
        drop_shadow: drop_fn::<Vec<(K, V)>>(),
        assemble: assemble_btree::<K, V>,
        settle: Some(settle::<K, V>),
        key_encode: <K as Encode>::ENCODE,
    }
}
