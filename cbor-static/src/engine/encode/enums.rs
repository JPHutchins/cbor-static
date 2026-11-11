//! Externally tagged enums: a unit variant is its name, any other `{name: payload}`.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, field_addr, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::{EncKind, FieldPtrFn, Layout};
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static EXTERNAL: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        layout,
        payloads,
        index,
        ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = unsafe { index(at) };
    match payloads.get(usize::from(variant)) {
        Some(None) => {
            state.pending.tail(layout.name_item(variant));
            Ok(None)
        }
        Some(Some(_)) => {
            state.pending.head(Major::Map, 1);
            state.pending.tail(layout.name_item(variant));
            push(
                frames,
                &mut state.depth,
                opened(kind, at, variant, Phase::EnumPayload),
            )
            .map(|()| None)
        }
        None => Err(ErrorKind::UnknownVariant),
    }
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Enum {
        layout,
        payloads,
        field_ptr,
        ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.phase != Phase::EnumPayload {
        return Ok(Part::Finished);
    }
    frame.phase = Phase::EnumDone;
    let payload = payloads
        .get(usize::from(frame.field))
        .copied()
        .flatten()
        .ok_or(ErrorKind::Unsupported)?;
    Ok(Part::Child(Child {
        kind: payload,
        at: payload_at(
            layout,
            *field_ptr,
            payload,
            frame.base.cast_const(),
            frame.field,
        ),
    }))
}

/// Where variant `variant`'s payload lives in the enum at `base`.
pub(super) fn payload_at(
    layout: &Layout,
    field_ptr: Option<FieldPtrFn>,
    payload: &EncKind,
    base: *const u8,
    variant: u8,
) -> *const u8 {
    match (field_ptr, payload) {
        (
            Some(_),
            EncKind::Struct {
                field_ptr: Some(_), ..
            },
        ) => base,
        (Some(glue), _) => unsafe { glue(base, 0) },
        (None, _) => field_addr(layout, None, base, variant),
    }
}
