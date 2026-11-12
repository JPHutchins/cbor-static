//! A sequence of up to a capacity of `T` (heapless `Vec`): an array of its elements. The frame's
//! `base` is the first element, `remaining` the length, and `seen` counts the elements written.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static SEQ: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Seq { view, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    let (first, len) = unsafe { view(at) };
    let len = u64::try_from(len).unwrap_or(u64::MAX);
    state.pending.head(Major::Array, len);
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining: len,
            ..opened(kind, first, 0, Phase::Value)
        },
    )
    .map(|()| None)
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Seq { elem, stride, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.seen >= frame.remaining {
        return Ok(Part::Finished);
    }
    let index = usize::try_from(frame.seen).map_err(|_| ErrorKind::Unsupported)?;
    frame.field = u8::try_from(index).unwrap_or(u8::MAX);
    frame.seen += 1;
    Ok(Part::Child(Child {
        kind: elem,
        at: frame.base.cast_const().wrapping_add(index * stride),
    }))
}
