#![allow(unsafe_code)]
//! The one module that dereferences descriptor offsets. Every other module forbids `unsafe`.

mod decode;
mod encode;
mod glue;
mod input;
mod leaf;
mod pending;
mod skip;
mod text;

use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr;

pub use decode::{Item, Next};
pub use glue::{assemble, drop_fn, field_ptr_of, index_of, index_tag, offset, write_tag};
pub(crate) use leaf::{
    DEC_BYTES, DEC_STR, ENC_BYTES, ENC_STR, absent_option, dec_i128, dec_option, dec_scalar,
    dec_tagged, dec_u128, enc_i128, enc_option, enc_scalar, enc_tagged, enc_u128, none,
};
pub use pending::Pending;

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
            state: encode::State::new(T::ENCODE, ptr::from_ref(value).cast()),
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

    /// The decoded value, once [`Decoder::feed`] has returned [`Fed::Done`].
    ///
    /// # Errors
    /// [`crate::ErrorKind::UnexpectedEnd`] before then.
    pub fn finish(mut self) -> Result<&'s mut T, Error> {
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
    1u64.checked_shl(u32::from(index)).unwrap_or(0)
}

fn frame_mut(frames: &mut [MaybeUninit<Frame>], index: u8) -> Result<&mut Frame, ErrorKind> {
    frames
        .get_mut(usize::from(index))
        .map(|slot| unsafe { slot.assume_init_mut() })
        .ok_or(ErrorKind::Unsupported)
}

fn push(frames: &mut [MaybeUninit<Frame>], depth: &mut u8, frame: Frame) -> Result<(), ErrorKind> {
    frames
        .get_mut(usize::from(*depth))
        .ok_or(ErrorKind::LimitExceeded)?
        .write(frame);
    *depth += 1;
    Ok(())
}
