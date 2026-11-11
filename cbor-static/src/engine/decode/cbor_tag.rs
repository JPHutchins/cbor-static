//! Enums discriminated by CBOR tag (`#[cbor(cbor_tag = N)]` on each variant, D-10): the tag
//! number picks the variant, whose payload is the tagged item.

use core::mem::MaybeUninit;

use super::enums::assembled;
use super::map::item_index;
use super::option::{done, holds_nothing, payload_only};
use super::{Begin, DecOps, Flow, State, at, expect, open, top, unnamed};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static CBOR_TAG: DecOps = DecOps {
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
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Enum {
        layout, variants, ..
    } = state.target.kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    if header.major != Major::Tag {
        return Err(ErrorKind::TypeMismatch);
    }
    let variant = item_index(layout, header).ok_or(ErrorKind::UnexpectedTag)?;
    let dst = state.target.dst;
    open(state, frames, Phase::EnumPayload, 0)?;
    top(state.depth, frames)?.field = variant;
    Ok(expect(
        state,
        variants
            .get(usize::from(variant))
            .and_then(|entry| entry.payload)
            .ok_or(ErrorKind::Unsupported)?,
        at(dst, layout, variant),
    ))
}
