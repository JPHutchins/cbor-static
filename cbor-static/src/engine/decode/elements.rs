//! `[T; N]`: an array of exactly `N` items of `T`, element `i` at `i * stride`. The frame's `seen`
//! counts the elements decoded.

use core::mem::MaybeUninit;

use super::array::{Element, element};
use super::{After, Begin, DecOps, Flow, State, desc, end, entries, expect, open, top, unnamed};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::{Frame, INDEFINITE, Phase};
use crate::wire::{Header, Major, length};

pub static ELEMENTS: DecOps = DecOps {
    value: Begin::Header(value),
    entry,
    named: unnamed,
    child_done,
    close,
    drop,
};

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Array { len, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    match (header.major, entries(header)?) {
        (Major::Array, count) if count == INDEFINITE || length(count) == Ok(*len) => {
            open(state, frames, Phase::Value, count)
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
    let DecKind::Array {
        elem, len, stride, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    match usize::try_from(frame.seen) {
        Ok(index) if index < *len => {
            Ok(expect(state, elem, frame.base.wrapping_add(index * stride)))
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

#[expect(clippy::unnecessary_wraps, reason = "DecOps::child_done can fail")]
fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    frame.seen += 1;
    frame.field = u8::try_from(frame.seen).unwrap_or(u8::MAX);
    Ok(After::Entry)
}

fn close(frame: &mut Frame) -> Result<(), ErrorKind> {
    match desc(frame) {
        DecKind::Array { len, .. } if usize::try_from(frame.seen) == Ok(*len) => Ok(()),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

/// Drops exactly the elements decoded.
fn drop(frame: &Frame) {
    if let DecKind::Array { elem, stride, .. } = desc(frame)
        && let Some(drop) = elem.drop_fn()
    {
        (0..usize::try_from(frame.seen).unwrap_or(0))
            .for_each(|index| unsafe { drop(frame.base.wrapping_add(index * stride)) });
    }
}
