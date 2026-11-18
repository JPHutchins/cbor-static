#![deny(unsafe_code)]
//! Containers that allocate, behind feature `alloc` (#5 Q8).

use alloc::string::String;
use alloc::vec::Vec;

use crate::engine::{
    BYTES_HEAP, ENCODE_SEQ, SEQ_HEAP, TEXT_HEAP, any_length, append_bytes, append_text, at_heap,
    drop_fn, empty, enc_text, enc_view, view_heap,
};
use crate::lead::Lead;
use crate::schema::{ByteString, DecKind, Decode, EncKind, Encode, Order, sealed};
use crate::stack::Level;

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
    const CONTIGUOUS: bool = false;
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
