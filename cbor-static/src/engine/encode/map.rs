//! Structs as maps whose keys are pre-encoded in the layout. With an `other` field (D-05b), its
//! captured entries take its place, each written as it arrived, under no key of its own.

use core::mem::MaybeUninit;

use super::{BEFORE_FIRST, Child, EncOps, Part, State, desc, field_addr, opened, present};
use crate::engine::FieldPtrFn;
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::keys::other_field;
use crate::schema::{EncKind, Layout};
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static MAP: EncOps = ops::<false>();
pub static MAP_OTHER: EncOps = ops::<true>();

const fn ops<const OTHER: bool>() -> EncOps {
    EncOps {
        open: open::<OTHER>,
        next: next::<OTHER>,
    }
}

fn open<const OTHER: bool>(
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
        if OTHER {
            with_captured(layout, kinds, *field_ptr, at)
        } else {
            present_count(layout, kinds, *field_ptr, at)
        },
    );
    push(
        frames,
        &mut state.depth,
        opened(kind, at, BEFORE_FIRST, Phase::Key),
    )
    .map(|()| None)
}

fn next<const OTHER: bool>(pending: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
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
            Some(field) if OTHER && other_field(layout) == Some(field) => {
                frame.field = field;
                Part::Child(Child {
                    kind: kinds
                        .get(usize::from(field))
                        .ok_or(ErrorKind::Unsupported)?,
                    at: field_addr(layout, *field_ptr, base, field),
                })
            }
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

/// How many entries the struct at `at` writes, its `other` field's captured ones counted in its
/// place.
fn with_captured(
    layout: &Layout,
    kinds: &[&EncKind],
    field_ptr: Option<FieldPtrFn>,
    at: *const u8,
) -> u64 {
    let count = present_count(layout, kinds, field_ptr, at);
    match other_field(layout) {
        Some(other) if present(layout, kinds, field_ptr, at, other) => {
            match kinds.get(usize::from(other)) {
                Some(EncKind::Seq { view, .. }) => (count - 1).saturating_add(
                    u64::try_from(unsafe { view(field_addr(layout, field_ptr, at, other)) }.1)
                        .unwrap_or(u64::MAX),
                ),
                _ => count,
            }
        }
        _ => count,
    }
}

/// How many of the struct at `at`'s fields are written.
pub(super) fn present_count(
    layout: &Layout,
    kinds: &[&EncKind],
    field_ptr: Option<FieldPtrFn>,
    at: *const u8,
) -> u64 {
    u64::try_from(
        (0..kinds.len())
            .filter_map(|f| u8::try_from(f).ok())
            .filter(|&f| present(layout, kinds, field_ptr, at, f))
            .count(),
    )
    .unwrap_or(u64::MAX)
}
