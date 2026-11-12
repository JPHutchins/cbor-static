#![deny(unsafe_code)]
//! Fixed-capacity containers from heapless 0.9, behind feature `heapless` (#12 Q19, Q21).

use heapless::{LenType, String, Vec};

use crate::engine::{
    BYTES_OWNED, TEXT_OWNED, any_length, append_string, append_vec, drop_fn, empty, enc_string,
    enc_view,
};
use crate::lead::Lead;
use crate::schema::{ByteString, DecKind, Decode, EncKind, Encode};
use crate::stack::Level;

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
#[allow(unsafe_code)]
unsafe impl<const N: usize, L: LenType> ByteString for Vec<u8, N, L> {
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
