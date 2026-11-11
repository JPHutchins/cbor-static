//! Structs as maps whose keys are pre-encoded in the layout.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, field_addr, opened, present};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

const BEFORE_FIRST: u8 = u8::MAX;

pub static MAP: EncOps = EncOps { open, next };

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
    state.pending.head(
        Major::Map,
        u64::try_from(
            (0..kinds.len())
                .filter_map(|f| u8::try_from(f).ok())
                .filter(|&f| present(layout, kinds, *field_ptr, at, f))
                .count(),
        )
        .unwrap_or(u64::MAX),
    );
    push(
        frames,
        &mut state.depth,
        opened(kind, at, BEFORE_FIRST, Phase::Key),
    )
    .map(|()| None)
}

fn next(pending: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Struct {
        layout,
        kinds,
        field_ptr,
        ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    let base = frame.base.cast_const();
    if frame.phase == Phase::Value {
        frame.phase = Phase::Key;
        return Ok(Part::Child(Child {
            kind: kinds
                .get(usize::from(frame.field))
                .ok_or(ErrorKind::Unsupported)?,
            at: field_addr(layout, *field_ptr, base, frame.field),
        }));
    }
    Ok(
        match (frame.field.wrapping_add(1)..u8::MAX)
            .take_while(|&f| usize::from(f) < kinds.len())
            .find(|&f| present(layout, kinds, *field_ptr, base, f))
        {
            Some(field) => {
                frame.field = field;
                frame.phase = Phase::Value;
                pending.tail(layout.name_item(field));
                Part::Wrote
            }
            None => Part::Finished,
        },
    )
}
