#![allow(unsafe_code)]
//! The one module that dereferences descriptor offsets. Every other module denies `unsafe`, except
//! to declare or implement the codec traits (D-24a).

#[cfg(feature = "heapless")]
mod capacity;
mod chunks;
mod decode;
mod encode;
mod glue;
mod input;
mod leaf;
mod pending;
mod raw;
mod skip;
mod storage;
mod strict;
mod strings;
mod text;

use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr;

#[cfg(feature = "heapless")]
pub(crate) use capacity::{assemble as assemble_map, at, entry, fresh, len_of, shadow_fits, view};
pub use chunks::{Chunk, ChunkIter, Chunks};
pub use decode::{
    ADJACENT as DECODE_ADJACENT, ARRAY as DECODE_ARRAY, AppendFn, BYTES_OWNED, BeginFn,
    CBOR_TAG as DECODE_CBOR_TAG, DEFAULTED as DECODE_DEFAULTED, DecOps,
    ELEMENTS as DECODE_ELEMENTS, EXTERNAL as DECODE_EXTERNAL, FinishFn, INDEX as DECODE_INDEX,
    INTERNAL as DECODE_INTERNAL, Item, MAP as DECODE_MAP, MAP_DENY as DECODE_MAP_DENY,
    MAP_INT as DECODE_MAP_INT, MAP_INT_DENY as DECODE_MAP_INT_DENY,
    MAP_INT_OTHER as DECODE_MAP_INT_OTHER, MAP_MIXED as DECODE_MAP_MIXED,
    MAP_MIXED_DENY as DECODE_MAP_MIXED_DENY, MAP_MIXED_OTHER as DECODE_MAP_MIXED_OTHER,
    MAP_OTHER as DECODE_MAP_OTHER, Next, OPTION as DECODE_OPTION, RAW as DECODE_RAW,
    SELF_DESCRIBED as DECODE_SELF_DESCRIBED, StoreFn, TEXT_OWNED, UNTAGGED as DECODE_UNTAGGED,
};
#[cfg(feature = "heapless")]
pub(crate) use decode::{LINEAR_MAP as DECODE_LINEAR_MAP, SEQ as DECODE_SEQ};
#[cfg(feature = "heapless")]
pub(crate) use encode::ENTRIES as ENCODE_ENTRIES;
pub use encode::{
    ADJACENT as ENCODE_ADJACENT, ARRAY as ENCODE_ARRAY, BYTEWISE, CBOR_TAG as ENCODE_CBOR_TAG,
    DeterministicOps, ELEMENTS as ENCODE_ELEMENTS, EXTERNAL as ENCODE_EXTERNAL, EncOps,
    INDEX as ENCODE_INDEX, INTERNAL as ENCODE_INTERNAL, LENGTH_FIRST, MAP as ENCODE_MAP,
    MAP_OTHER as ENCODE_MAP_OTHER, OPTION as ENCODE_OPTION, TAG as ENCODE_TAG,
    UNTAGGED as ENCODE_UNTAGGED,
};
use encode::{BYTE_CHUNKS, TEXT_CHUNKS};
#[cfg(feature = "heapless")]
pub(crate) use encode::{LINEAR_MAP as ENCODE_LINEAR_MAP, SEQ as ENCODE_SEQ};
pub use glue::{
    AtFn, EntryFn, FieldPtrFn, FreshFn, IndexFn, LenFn, SomeFn, ViewFn, WriteFn, assemble, drop_fn,
    field_ptr_of, index_of, index_tag, offset, some, some_ptr, wraps, write_default, write_made,
    write_tag,
};
pub(crate) use input::Input;
pub use leaf::ENC_SKIPPED as ENCODE_SKIPPED;
pub(crate) use leaf::{
    AbsentFn, DEC_BYTES, DEC_STR, DecodeFn, ENC_BYTES, ENC_STR, EncodeFn, NULL_ITEM, absent_option,
    dec_i128, dec_option, dec_scalar, dec_skipped, dec_tagged, dec_u128, enc_i128, enc_option,
    enc_scalar, enc_u128, none,
};
pub use pending::Pending;
pub use raw::{ArrayIter, ArrayRef, MapIter, MapRef, RawCbor, RawEntry, Skipped};
pub(crate) use skip::Skip;
#[cfg(test)]
pub(crate) use skip::item_len;
pub use storage::{as_slice, filled, gathered, get, set};
use strict::STRICT;
pub(crate) use strict::StrictOps;
pub(crate) use strings::{DEC_CHAR, ENC_CHAR, enc_view, exactly, nothing, within};
#[cfg(feature = "heapless")]
pub(crate) use strings::{any_length, append_string, append_vec, empty, enc_string};
pub(crate) use text::utf8;

use crate::error::{Error, ErrorKind};
use crate::schema::{Decode, Encode};
use crate::stack::{Frame, fits};

/// Encoding as far as the last output buffer allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Done(usize),
    NeedSpace(usize),
}

/// Decoding as far as the last input chunk allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fed {
    Done(usize),
    NeedInput,
}

/// Resumable encoder: each [`Encoder::resume`] fills one output buffer.
///
/// ```
/// let mut encoder = cbor_static::Encoder::new(&1_000_000u32);
/// let mut chunk = [0u8; 2];
/// assert_eq!(encoder.resume(&mut chunk)?, cbor_static::Progress::NeedSpace(2));
/// assert_eq!(chunk, [0x1a, 0x00]);
/// assert_eq!(encoder.resume(&mut chunk)?, cbor_static::Progress::NeedSpace(2));
/// assert_eq!(encoder.resume(&mut chunk)?, cbor_static::Progress::Done(1));
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub struct Encoder<'v, T: Encode> {
    state: encode::State,
    stack: MaybeUninit<T::Stack>,
    value: PhantomData<&'v T>,
}

/// Resumable decoder into a caller-owned slot: each [`Decoder::feed`] consumes one chunk.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::<u64>::uninit();
/// let mut decoder = cbor_static::Decoder::new(&mut slot);
/// assert_eq!(decoder.feed(&[0x1b, 0, 0, 0])?, cbor_static::Fed::NeedInput);
/// assert_eq!(decoder.feed(&[1, 0, 0, 0, 0])?, cbor_static::Fed::Done(5));
/// assert_eq!(*decoder.finish()?, 1 << 32);
/// assert!(decoder.finish().is_err());
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub struct Decoder<'de, 's, T: Decode<'de>> {
    state: decode::State,
    stack: MaybeUninit<T::Stack>,
    slot: PhantomData<(&'s mut T, &'de [u8])>,
}

impl<'v, T: Encode> Encoder<'v, T> {
    pub fn new(value: &'v T) -> Self {
        Self {
            state: match T::DETERMINISTIC {
                None => encode::State::new(T::ENCODE, ptr::from_ref(value).cast()),
                Some(ops) => {
                    encode::State::deterministic(T::ENCODE, ptr::from_ref(value).cast(), ops)
                }
            },
            stack: MaybeUninit::uninit(),
            value: PhantomData,
        }
    }

    /// Writes the next part of the encoding into `out`.
    ///
    /// # Errors
    /// None for derived types; an error indicates a corrupt descriptor.
    pub fn resume(&mut self, out: &mut [u8]) -> Result<Progress, Error> {
        const { assert!(fits::<T::Stack>(T::MAX_DEPTH)) };
        encode::resume(&mut self.state, frames(&mut self.stack, T::MAX_DEPTH), out)
    }
}

impl<'de, 's, T: Decode<'de>> Decoder<'de, 's, T> {
    pub fn new(slot: &'s mut MaybeUninit<T>) -> Self {
        Self {
            state: decode::State::new(T::DECODE, slot.as_mut_ptr().cast()),
            stack: MaybeUninit::uninit(),
            slot: PhantomData,
        }
    }

    /// A decoder under the `Strict` profile (D-12): it accepts only what RFC 8949 §4.2.1's
    /// deterministic encoding writes, and rejects entries no field claims. Key order is checked
    /// against the type's own keys, so input may still arrive in chunks.
    ///
    /// ```
    /// #[derive(cbor_static::Decode, Debug, PartialEq)]
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    ///
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let mut decoder = cbor_static::Decoder::<Point>::new_strict(&mut slot);
    /// assert_eq!(decoder.feed(b"\xa2\x61x")?, cbor_static::Fed::NeedInput);
    /// assert_eq!(decoder.feed(b"\x01\x61y\x02")?, cbor_static::Fed::Done(4));
    /// assert_eq!(*decoder.finish()?, Point { x: 1, y: 2 });
    ///
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let mut decoder = cbor_static::Decoder::<Point>::new_strict(&mut slot);
    /// assert_eq!(decoder.feed(b"\xa2\x61y\x02").map(|_| ()), Ok(()));
    /// assert_eq!(
    ///     decoder.feed(b"\x61x\x01").map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::NotDeterministic)
    /// );
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    pub fn new_strict(slot: &'s mut MaybeUninit<T>) -> Self {
        Self {
            state: decode::State::strict(T::DECODE, slot.as_mut_ptr().cast()),
            stack: MaybeUninit::uninit(),
            slot: PhantomData,
        }
    }

    /// A decoder under the profile `strict` names, which a raw view's own decoder passes on to the
    /// decoders that check its elements, so firmware without `Strict` links none of it (D-22).
    pub(crate) fn under(slot: &'s mut MaybeUninit<T>, strict: Option<&'static StrictOps>) -> Self {
        Self {
            state: decode::State::under(T::DECODE, slot.as_mut_ptr().cast(), strict),
            stack: MaybeUninit::uninit(),
            slot: PhantomData,
        }
    }

    /// Consumes `chunk`, returning how much of it the item used once complete.
    ///
    /// # Errors
    /// Any decode error; the fields decoded so far are dropped and later calls repeat it.
    pub fn feed(&mut self, chunk: &'de [u8]) -> Result<Fed, Error> {
        const { assert!(fits::<T::Stack>(T::MAX_DEPTH)) };
        decode::feed(
            &mut self.state,
            frames(&mut self.stack, T::MAX_DEPTH),
            chunk,
        )
    }

    /// The decoded value, once [`Decoder::feed`] has returned [`Fed::Done`]; at most once.
    ///
    /// # Errors
    /// [`crate::ErrorKind::UnexpectedEnd`] before then; an error on every later call.
    pub fn finish(&mut self) -> Result<&'s mut T, Error> {
        decode::take(&mut self.state).map(|root| unsafe { &mut *root.cast::<T>() })
    }
}

impl<'de, T: Decode<'de>> Drop for Decoder<'de, '_, T> {
    fn drop(&mut self) {
        decode::abandon(&mut self.state, frames(&mut self.stack, T::MAX_DEPTH));
    }
}

fn frames<S>(stack: &mut MaybeUninit<S>, depth: usize) -> &mut [MaybeUninit<Frame>] {
    if depth == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(stack.as_mut_ptr().cast(), depth) }
    }
}

pub(crate) fn bit(index: u8) -> u64 {
    #[cfg(kani)]
    kani::assert(index < 64, "a bit index names one of 64 levels");
    1u64.checked_shl(u32::from(index)).unwrap_or(0)
}

fn frame_mut(frames: &mut [MaybeUninit<Frame>], index: u8) -> Result<&mut Frame, ErrorKind> {
    frames
        .get_mut(usize::from(index))
        .map(|slot| unsafe { slot.assume_init_mut() })
        .ok_or(ErrorKind::Unsupported)
}

fn top_field(frames: &mut [MaybeUninit<Frame>], depth: u8) -> u8 {
    depth
        .checked_sub(1)
        .and_then(|top| frame_mut(frames, top).ok())
        .map_or(0, |frame| frame.field)
}

fn push(frames: &mut [MaybeUninit<Frame>], depth: &mut u8, frame: Frame) -> Result<(), ErrorKind> {
    frames
        .get_mut(usize::from(*depth))
        .ok_or(ErrorKind::LimitExceeded)?
        .write(frame);
    *depth += 1;
    Ok(())
}

#[cfg(test)]
mod tests;
