//! `[T; N]`: an array of its `N` elements. The frame's `seen` counts the elements written.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static ELEMENTS: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Array { len, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    state
        .pending
        .head(Major::Array, u64::try_from(*len).unwrap_or(u64::MAX));
    push(frames, &mut state.depth, opened(kind, at, 0, Phase::Value)).map(|()| None)
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Array {
        elem, len, stride, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    match usize::try_from(frame.seen) {
        Ok(index) if index < *len => {
            frame.field = u8::try_from(index).unwrap_or(u8::MAX);
            frame.seen += 1;
            Ok(Part::Child(Child {
                kind: elem,
                at: frame.base.cast_const().wrapping_add(index * stride),
            }))
        }
        _ => Ok(Part::Finished),
    }
}
