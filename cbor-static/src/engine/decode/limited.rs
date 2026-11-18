//! The string, sequence and map forms of a value whose length `#[cbor(max_len = N)]` bounds
//! (#5 Q15). Each rejects a definite length over `N` at its head, before anything is copied; the
//! value's own `append` or `at` bounds an indefinite one as its chunks or elements arrive.

use core::mem::MaybeUninit;

use super::{Begin, DecOps, Flow, State, linear_map, owned, seq};
use crate::engine::input::Input;
use crate::engine::limited::fits;
use crate::error::ErrorKind;
use crate::stack::Frame;
use crate::wire::Header;

/// The forms, each bounded to `N` bytes, elements or entries.
pub struct Limited<const N: usize>;

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
        ..seq::SEQ
    };
    pub const LINEAR_MAP: DecOps = DecOps {
        value: Begin::Header(map::<N>),
        ..linear_map::LINEAR_MAP
    };
}

fn text<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header)?;
    owned::text(state, frames, input, header)
}

fn bytes<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header)?;
    owned::bytes(state, frames, input, header)
}

fn sequence<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header)?;
    seq::value(state, frames, input, header)
}

fn map<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    fits::<N>(header)?;
    linear_map::value(state, frames, input, header)
}
