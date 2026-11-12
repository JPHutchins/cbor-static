//! A sequence of up to a capacity of `T` (heapless `Vec`): an array whose elements decode in place,
//! each committed to the sequence's length as it completes, so the sequence's own drop drops
//! exactly what was decoded (D-17). The frame's `seen` counts the elements decoded.

use core::mem::MaybeUninit;

use super::array::{Element, element};
use super::option::done;
use super::{After, Begin, DecOps, Flow, State, desc, end, entries, expect, open, top, unnamed};
use crate::engine::AtFn;
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static SEQ: DecOps = DecOps {
    value: Begin::Header(value),
    entry,
    named: unnamed,
    child_done,
    close: done,
    drop,
};

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Seq { begin, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    match header.major {
        Major::Array => {
            unsafe { begin(state.target.dst) };
            open(state, frames, Phase::Value, entries(header)?)
        }
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
    let DecKind::Seq { elem, at, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    Ok(expect(state, elem, slot(*at, frame.base, frame.seen)?))
}

#[expect(clippy::unnecessary_wraps, reason = "DecOps::child_done can fail")]
fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    frame.seen += 1;
    frame.field = u8::try_from(frame.seen).unwrap_or(u8::MAX);
    if let DecKind::Seq { at, .. } = desc(frame) {
        unsafe { at(frame.base, index(frame.seen)) };
    }
    Ok(After::Entry)
}

fn drop(frame: &Frame) {
    if let DecKind::Seq {
        drop: Some(drop), ..
    } = desc(frame)
    {
        unsafe { drop(frame.base) };
    }
}

/// The slot of element `len`, after committing the `len` before it.
///
/// # Errors
/// [`ErrorKind::OutOfRange`] at capacity.
pub(super) fn slot(at: AtFn, base: *mut u8, len: u64) -> Result<*mut u8, ErrorKind> {
    let slot = unsafe { at(base, index(len)) };
    if slot.is_null() {
        Err(ErrorKind::OutOfRange)
    } else {
        Ok(slot)
    }
}

pub(super) fn index(count: u64) -> usize {
    usize::try_from(count).unwrap_or(usize::MAX)
}
