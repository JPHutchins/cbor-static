//! The string, sequence and map forms of a value whose length `#[cbor(max_len = N)]` bounds
//! (#5 Q15). Each rejects a definite length over `N` at its head, before anything is copied; the
//! value's own `append` or `at` bounds an indefinite one as its chunks or elements arrive.

use core::mem::MaybeUninit;

#[cfg(any(feature = "heapless", feature = "alloc"))]
use super::linear_map;
#[cfg(feature = "heapless")]
use super::{Begin, DecOps};
use super::{Flow, State, owned, seq, top};
use crate::engine::input::Input;
use crate::engine::limited::fits;
use crate::error::ErrorKind;
use crate::stack::Frame;
use crate::wire::{Header, Major};

/// The forms, each bounded to `N` bytes, elements or entries.
#[cfg(feature = "heapless")]
pub struct Limited<const N: usize>;

#[cfg(feature = "heapless")]
impl<const N: usize> Limited<N> {
    pub const TEXT: DecOps = DecOps {
        value: Begin::Header(text::<N>),
        ..owned::TEXT
    };
    pub const BYTES: DecOps = DecOps {
        value: Begin::Header(bytes::<N>),
        ..owned::BYTES
    };
    pub const SEQ: DecOps = DecOps {
        value: Begin::Header(sequence::<N>),
        entry: element::<N>,
        ..seq::SEQ
    };
    pub const LINEAR_MAP: DecOps = DecOps {
        value: Begin::Header(map::<N>),
        entry: entry::<N>,
        ..linear_map::LINEAR_MAP
    };
}

pub(super) fn text<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header, Major::Text)?;
    owned::text(state, frames, input, header)
}

pub(super) fn bytes<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header, Major::Bytes)?;
    owned::bytes(state, frames, input, header)
}

pub(super) fn sequence<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header, Major::Array)?;
    seq::value(state, frames, input, header)
}

#[cfg(any(feature = "heapless", feature = "alloc"))]
pub(super) fn map<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header, Major::Map)?;
    linear_map::value(state, frames, input, header)
}

/// [`seq::entry`], whose element `N` is over the limit, not past the capacity.
#[cfg(feature = "heapless")]
fn element<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    past::<N>(seq::entry(state, frames, input), state, frames)
}

#[cfg(feature = "heapless")]
/// [`linear_map::entry`], whose entry `N` is over the limit, not past the capacity.
fn entry<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    past::<N>(linear_map::entry(state, frames, input), state, frames)
}

/// `step`, but a slot refused once the top frame holds `N` items is over the limit: `at_limited`
/// refuses no earlier one.
pub(super) fn past<const N: usize>(
    step: Result<Flow, ErrorKind>,
    state: &State,
    frames: &mut [MaybeUninit<Frame>],
) -> Result<Flow, ErrorKind> {
    match step {
        Err(ErrorKind::OutOfRange)
            if top(state.depth, frames).is_ok_and(|frame| seq::index(frame.seen) >= N) =>
        {
            Err(ErrorKind::LimitExceeded)
        }
        step => step,
    }
}
