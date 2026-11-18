//! Containers that allocate (feature `alloc`): strings copied as their payload arrives, under a
//! frame whose drop frees what the copy had grown when decoding fails partway (D-17), and
//! sequences whose every slot is reserved as its element begins (#5 Q9, Q10).

use core::mem::MaybeUninit;

use super::limited::{bytes as bytes_within, past, sequence, text as text_within};
use super::option::{done, payload_only};
use super::{After, Begin, DecOps, Flow, State, desc, owned, seq, unnamed};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::Frame;
use crate::wire::Header;

pub static TEXT: DecOps = DecOps {
    value: Begin::Header(text),
    entry: payload_only,
    named: unnamed,
    child_done: copied,
    close: done,
    drop,
};

pub static BYTES: DecOps = DecOps {
    value: Begin::Header(bytes),
    ..TEXT
};

pub static SEQ: DecOps = DecOps {
    value: Begin::Header(seq::value),
    entry: element,
    named: unnamed,
    child_done: seq::child_done,
    close: done,
    drop: seq::drop,
};

/// The forms, each bounded to `N` bytes or elements.
pub struct Limited<const N: usize>;

impl<const N: usize> Limited<N> {
    pub const TEXT: DecOps = DecOps {
        value: Begin::Header(text_limited::<N>),
        ..TEXT
    };
    pub const BYTES: DecOps = DecOps {
        value: Begin::Header(bytes_limited::<N>),
        ..TEXT
    };
    pub const SEQ: DecOps = DecOps {
        value: Begin::Header(sequence::<N>),
        entry: element_limited::<N>,
        ..seq::SEQ
    };
}

fn text(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    owned::text(state, frames, input, header)?;
    framed(state, frames)
}

fn bytes(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    owned::bytes(state, frames, input, header)?;
    framed(state, frames)
}

fn text_limited<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    text_within::<N>(state, frames, input, header)?;
    framed(state, frames)
}

fn bytes_limited<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    bytes_within::<N>(state, frames, input, header)?;
    framed(state, frames)
}

/// Holds the string the copy has just begun under a frame of its own, which errors still report as
/// the enclosing field.
fn framed(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<Flow, ErrorKind> {
    use core::ptr;

    use crate::engine::{push, top_field};
    use crate::stack::Phase;

    let field = top_field(frames, state.depth);
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining: 0,
            seen: 0,
            desc: ptr::from_ref(state.target.kind).cast(),
            base: state.target.dst,
            field,
            phase: Phase::Value,
        },
    )?;
    Ok(Flow::Continue)
}

/// The copy is complete, and the string with it.
#[expect(clippy::unnecessary_wraps, reason = "DecOps::child_done can fail")]
const fn copied(_: &mut Frame) -> Result<After, ErrorKind> {
    Ok(After::Close)
}

fn drop(frame: &Frame) {
    if let DecKind::Owned {
        drop: Some(drop), ..
    } = desc(frame)
    {
        unsafe { drop(frame.base) };
    }
}

/// [`seq::entry`], whose slot is refused only when it cannot be reserved.
fn element(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    unreserved(seq::entry(state, frames, input))
}

/// [`seq::entry`], whose element `N` is over the limit, and any earlier slot refused is one that
/// cannot be reserved.
fn element_limited<const N: usize>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    unreserved(past::<N>(seq::entry(state, frames, input), state, frames))
}

fn unreserved(step: Result<Flow, ErrorKind>) -> Result<Flow, ErrorKind> {
    match step {
        Err(ErrorKind::OutOfRange) => Err(ErrorKind::OutOfMemory),
        step => step,
    }
}
