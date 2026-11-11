//! Structs as positional arrays (`#[cbor(array)]`, D-09) and tuples: element `i` is field `i`, and
//! fields after the last element take their defaults.

use core::mem::MaybeUninit;

use super::{After, Begin, DecOps, Flow, State, at, desc, end, entries, expect, open, top};
use crate::engine::bit;
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::{Frame, INDEFINITE, Phase};
use crate::wire::{Header, INDEFINITE as BREAK_INFO, Major};

pub static ARRAY: DecOps = DecOps {
    value: Begin::Header(value),
    entry,
    named: super::unnamed,
    child_done,
    close: super::map::close,
    drop: super::map::drop,
};

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    match header.major {
        Major::Array => open(state, frames, Phase::Value, entries(header)?),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    match element(state, frames, input)? {
        Element::Wait => return Ok(Flow::NeedInput),
        Element::End => return end(state, frames),
        Element::Next => {}
    }
    let frame = top(state.depth, frames)?;
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    Ok(expect(
        state,
        kinds
            .get(usize::from(frame.field))
            .ok_or(ErrorKind::TypeMismatch)?,
        at(frame.base, layout, frame.field),
    ))
}

#[expect(clippy::unnecessary_wraps, reason = "DecOps::child_done can fail")]
fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    frame.seen |= bit(frame.field);
    frame.field += 1;
    Ok(After::Entry)
}

/// Whether the top frame's array has another element, which the value step reads.
pub(super) enum Element {
    Next,
    End,
    Wait,
}

/// Counts the top frame's next element, or takes the break that ends an indefinite array.
pub(super) fn element(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Element, ErrorKind> {
    let frame = top(state.depth, frames)?;
    match (frame.remaining, state.carry.initial(input)) {
        (0, _) => Ok(Element::End),
        (INDEFINITE, None) => Ok(Element::Wait),
        (INDEFINITE, Some(BREAK)) => state.carry.header(input).map(|_| Element::End),
        (INDEFINITE, Some(_)) => Ok(Element::Next),
        (_, _) => {
            frame.remaining -= 1;
            Ok(Element::Next)
        }
    }
}

const BREAK: u8 = (Major::Simple as u8) << 5 | BREAK_INFO;
