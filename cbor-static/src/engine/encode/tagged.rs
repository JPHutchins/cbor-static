//! Internally tagged enums, `{tag: name, ...payload fields}`, and adjacently tagged ones,
//! `{tag: name, content: payload}` with no content for a unit variant (D-10). The layout holds the
//! variant names, then the tag key, then the content key.

use core::mem::MaybeUninit;

use super::enums::{next as payload_next, payload_at};
use super::map::present_count;
use super::{BEFORE_FIRST, Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static INTERNAL: EncOps = EncOps {
    open: internal,
    next: internal_next,
};

pub static ADJACENT: EncOps = EncOps {
    open: adjacent,
    next: adjacent_next,
};

fn internal(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        layout,
        payloads,
        index,
        field_ptr,
        ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = unsafe { index(at) };
    let fields = match payloads
        .get(usize::from(variant))
        .ok_or(ErrorKind::UnknownVariant)?
    {
        None => 0,
        Some(
            payload @ EncKind::Struct {
                layout: fields,
                kinds,
                field_ptr: payload_ptr,
                ..
            },
        ) => present_count(
            fields,
            kinds,
            *payload_ptr,
            payload_at(layout, *field_ptr, payload, at, variant),
        ),
        Some(_) => return Err(ErrorKind::Unsupported),
    };
    state.pending.head(Major::Map, 1 + fields);
    open_tagged(state, frames, kind, at, variant)
}

/// After the variant name, the frame becomes the payload struct's map frame, whose fields follow
/// in the same map.
fn internal_next(pending: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Enum {
        layout,
        payloads,
        field_ptr,
        ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.phase != Phase::EnumName {
        return Ok(Part::Finished);
    }
    pending.tail(layout.name_item(frame.field));
    match payloads.get(usize::from(frame.field)).copied().flatten() {
        Some(payload) => {
            *frame = opened(
                payload,
                payload_at(
                    layout,
                    *field_ptr,
                    payload,
                    frame.base.cast_const(),
                    frame.field,
                ),
                BEFORE_FIRST,
                Phase::Key,
            );
        }
        None => frame.phase = Phase::EnumDone,
    }
    Ok(Part::Wrote)
}

fn adjacent(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        payloads, index, ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = unsafe { index(at) };
    let has_content = payloads
        .get(usize::from(variant))
        .ok_or(ErrorKind::UnknownVariant)?
        .is_some();
    state.pending.head(Major::Map, 1 + u64::from(has_content));
    open_tagged(state, frames, kind, at, variant)
}

/// The variant name, then the content key, then the payload as the external form writes it.
fn adjacent_next(pending: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Enum {
        layout, payloads, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    match frame.phase {
        Phase::EnumName => {
            pending.tail(layout.name_item(frame.field));
            frame.phase = match payloads.get(usize::from(frame.field)) {
                Some(Some(_)) => Phase::Key,
                _ => Phase::EnumDone,
            };
            Ok(Part::Wrote)
        }
        Phase::Key => {
            pending.tail(layout.name_item(content_key(payloads.len())?));
            frame.phase = Phase::EnumPayload;
            Ok(Part::Wrote)
        }
        _ => payload_next(pending, frame),
    }
}

/// Writes the tag key and opens the frame that writes the variant name next.
fn open_tagged(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
    variant: u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        layout, payloads, ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    state
        .pending
        .tail(layout.name_item(content_key(payloads.len())? - 1));
    push(
        frames,
        &mut state.depth,
        opened(kind, at, variant, Phase::EnumName),
    )
    .map(|()| None)
}

/// The layout entry of the content key, after the variants and the tag key.
fn content_key(variants: usize) -> Result<u8, ErrorKind> {
    u8::try_from(variants + 1).map_err(|_| ErrorKind::Unsupported)
}
