//! `Option<T>` of a nested `T`: an untagged choice between null (`None`) and `T`, made on the
//! initial byte. `T` decodes into the slot, then moves into `Some`; frame `field` 0 is `None`.

use core::mem::MaybeUninit;

use super::{After, Begin, DecOps, Flow, State, desc, expect, open, top, unnamed};
use crate::engine::NULL_ITEM;
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::{Frame, Phase};
use crate::wire::{Major, NULL};

pub static OPTION: DecOps = DecOps {
    value: Begin::Peek(value),
    entry: payload_only,
    named: unnamed,
    child_done,
    close: done,
    drop: holds_nothing,
};

const NULL_INITIAL: u8 = (Major::Simple as u8) << 5 | NULL;

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    initial: u8,
) -> Result<Flow, ErrorKind> {
    let DecKind::Option { some, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    let dst = state.target.dst;
    open(state, frames, Phase::EnumPayload, 0)?;
    let some_follows = initial != NULL_INITIAL;
    top(state.depth, frames)?.field = u8::from(some_follows);
    Ok(expect(
        state,
        if some_follows { some } else { &NULL_ITEM },
        dst,
    ))
}

/// The frame never takes an entry of its own: its payload completes it.
fn payload_only(
    _: &mut State,
    _: &mut [MaybeUninit<Frame>],
    _: &mut Input,
) -> Result<Flow, ErrorKind> {
    Err(ErrorKind::Unsupported)
}

fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    match (desc(frame), frame.field) {
        (
            DecKind::Option {
                default: Some(none),
                ..
            },
            0,
        ) => unsafe { none(frame.base) },
        (DecKind::Option { assemble, .. }, _) => unsafe { assemble(frame.base) },
        _ => return Err(ErrorKind::Unsupported),
    }
    frame.phase = Phase::EnumDone;
    Ok(After::Close)
}

#[expect(clippy::unnecessary_wraps, reason = "DecOps::close can fail")]
const fn done(_: &mut Frame) -> Result<(), ErrorKind> {
    Ok(())
}

/// The value is written whole when its payload completes, so a pending frame holds nothing.
const fn holds_nothing(_: &Frame) {}
