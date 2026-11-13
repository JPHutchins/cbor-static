//! Restricted untagged enums (D-10): the item's head picks the one variant whose payload can start
//! with it, by its initial byte's class and, for a tag, by its tag number (#13 Q9); a unit variant
//! is null. The layout stores each variant's [`Lead`]. The head is given back, so the payload reads
//! it again as its own; the payload decodes into the slot, then `assemble` builds the variant.

use core::mem::MaybeUninit;

use super::described::give_back;
use super::enums::assembled;
use super::option::{done, holds_nothing, payload_only};
use super::{Begin, DecOps, Flow, State, at, expect, open, top, unnamed};
use crate::engine::NULL_ITEM;
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::schema::{DecKind, Layout};
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static UNTAGGED: DecOps = DecOps {
    value: Begin::Header(value),
    entry: payload_only,
    named: unnamed,
    child_done: assembled,
    close: done,
    drop: holds_nothing,
};

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Enum {
        layout, variants, ..
    } = state.target.kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let starts = match header.major {
        Major::Tag => Lead::tagged(header.arg),
        _ => Lead::of(header.initial()),
    };
    let variant = (0..variants.len())
        .filter_map(|index| u8::try_from(index).ok())
        .find(|&index| accepts(layout, index).overlaps(starts))
        .ok_or(ErrorKind::TypeMismatch)?;
    give_back(state, input, header)?;
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

/// The leads variant `index`'s payload can start with.
fn accepts(layout: &Layout, index: u8) -> Lead {
    layout
        .fields
        .get(usize::from(index))
        .and_then(|place| layout.names.get(usize::from(place.name_at)..))
        .and_then(|bytes| bytes.first_chunk::<{ Lead::ENCODED_LEN }>())
        .map_or(Lead::NONE, |bytes| Lead::from_le_bytes(*bytes))
}
