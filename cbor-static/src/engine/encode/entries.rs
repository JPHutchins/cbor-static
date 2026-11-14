//! An `other` field's captured entries (R-128), in its struct's map: each key, then its value, as
//! they arrived (D-11a), with no head of their own, since the map's head counts them. The frame's
//! `base` is the first entry, `remaining` the count, `seen` the entries written, and its phase
//! whether an entry's key or value is next.

use core::mem::{MaybeUninit, offset_of};

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::RawEntry;
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};

pub static ENTRIES: EncOps = EncOps { open, next };

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
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining: u64::try_from(len).unwrap_or(u64::MAX),
            ..opened(kind, first, 0, Phase::Key)
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
    let entry = frame.base.cast_const().wrapping_add(index * stride);
    frame.field = u8::try_from(index).unwrap_or(u8::MAX);
    Ok(Part::Child(Child {
        kind: elem,
        at: if frame.phase == Phase::Key {
            frame.phase = Phase::Value;
            entry.wrapping_add(offset_of!(RawEntry<'static>, key))
        } else {
            frame.phase = Phase::Key;
            frame.seen += 1;
            entry.wrapping_add(offset_of!(RawEntry<'static>, value))
        },
    }))
}
