//! Structs as positional arrays (`#[cbor(array)]`, D-09) and tuples: every field up to the last
//! present one, so trailing absent `Option`s are truncated and earlier ones are null.

use core::mem::MaybeUninit;

use super::{BEFORE_FIRST, Child, EncOps, Part, State, desc, field_addr, opened, present};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static ARRAY: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Struct {
        layout,
        kinds,
        field_ptr,
        ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let len = (0..kinds.len())
        .filter_map(|f| u8::try_from(f).ok())
        .rev()
        .find(|&f| present(layout, kinds, *field_ptr, at, f))
        .map_or(0, |last| u64::from(last) + 1);
    state.pending.head(Major::Array, len);
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining: len,
            ..opened(kind, at, BEFORE_FIRST, Phase::Value)
        },
    )
    .map(|()| None)
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Struct {
        layout,
        kinds,
        field_ptr,
        ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    let field = frame.field.wrapping_add(1);
    if u64::from(field) >= frame.remaining {
        return Ok(Part::Finished);
    }
    frame.field = field;
    Ok(Part::Child(Child {
        kind: kinds
            .get(usize::from(field))
            .ok_or(ErrorKind::Unsupported)?,
        at: field_addr(layout, *field_ptr, frame.base.cast_const(), field),
    }))
}
