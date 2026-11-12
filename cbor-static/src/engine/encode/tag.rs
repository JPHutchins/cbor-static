//! A value wrapped in a CBOR tag: the tag's head, then the value itself. The head drains before
//! the value begins, so a value as long as `Pending` holds still fits after any tag.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static TAG: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Tagged { tag, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    state.pending.head(Major::Tag, **tag);
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining: 1,
            ..opened(kind, at, 0, Phase::Value)
        },
    )
    .map(|()| None)
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Tagged { inner, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.seen >= frame.remaining {
        return Ok(Part::Finished);
    }
    frame.seen += 1;
    Ok(Part::Child(Child {
        kind: inner,
        at: frame.base.cast_const(),
    }))
}
