//! Restricted untagged enums (D-10): the initial byte's class picks the one variant whose payload
//! can start with it, and a unit variant is null. The layout stores each variant's classes. The
//! payload decodes into the slot, then `assemble` builds the variant.

use core::mem::MaybeUninit;

use super::option::{done, holds_nothing, payload_only};
use super::{After, Begin, DecOps, Flow, State, at, desc, expect, open, top, unnamed};
use crate::engine::NULL_ITEM;
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::schema::{DecKind, Layout};
use crate::stack::{Frame, Phase};

pub static UNTAGGED: DecOps = DecOps {
    value: Begin::Peek(value),
    entry: payload_only,
    named: unnamed,
    child_done,
    close: done,
    drop: holds_nothing,
};

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    initial: u8,
) -> Result<Flow, ErrorKind> {
    let DecKind::Enum {
        layout, variants, ..
    } = state.target.kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = (0..variants.len())
        .filter_map(|index| u8::try_from(index).ok())
        .find(|&index| accepts(layout, index).overlaps(Lead::of(initial)))
        .ok_or(ErrorKind::TypeMismatch)?;
    let dst = state.target.dst;
    open(state, frames, Phase::EnumPayload, 0)?;
    top(state.depth, frames)?.field = variant;
    Ok(expect(
        state,
        variants
            .get(usize::from(variant))
            .ok_or(ErrorKind::Unsupported)?
            .payload
            .unwrap_or(&NULL_ITEM),
        at(dst, layout, variant),
    ))
}

fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    let DecKind::Enum { variants, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = variants
        .get(usize::from(frame.field))
        .ok_or(ErrorKind::Unsupported)?;
    unsafe { (variant.assemble)(frame.base) };
    frame.phase = Phase::EnumDone;
    Ok(After::Close)
}

/// The classes variant `index`'s payload can start with.
fn accepts(layout: &Layout, index: u8) -> Lead {
    layout
        .fields
        .get(usize::from(index))
        .and_then(|place| layout.names.get(usize::from(place.name_at)..))
        .and_then(|bytes| bytes.first_chunk::<2>())
        .map_or(Lead::NONE, |bytes| Lead::from_le_bytes(*bytes))
}
