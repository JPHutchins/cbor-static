//! A map of up to a capacity of entries (heapless `LinearMap`): a map of its entries in their order.
//! The frame's `seen` counts whole entries; its phase says whether entry `seen`'s key or value is
//! next.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static LINEAR_MAP: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Map { len, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    state.pending.head(
        Major::Map,
        u64::try_from(unsafe { len(at) }).unwrap_or(u64::MAX),
    );
    push(frames, &mut state.depth, opened(kind, at, 0, Phase::Key)).map(|()| None)
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Map {
        key, value, entry, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    let index = usize::try_from(frame.seen).map_err(|_| ErrorKind::Unsupported)?;
    let (at_key, at_value) = unsafe { entry(frame.base, index) };
    if at_key.is_null() {
        return Ok(Part::Finished);
    }
    frame.field = u8::try_from(index).unwrap_or(u8::MAX);
    Ok(Part::Child(if frame.phase == Phase::Key {
        frame.phase = Phase::Value;
        Child {
            kind: key,
            at: at_key,
        }
    } else {
        frame.phase = Phase::Key;
        frame.seen += 1;
        Child {
            kind: value,
            at: at_value,
        }
    }))
}
