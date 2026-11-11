//! Structs as maps with text keys.

use core::mem::MaybeUninit;

use super::{
    After, DecOps, Entry, Flow, State, at, desc, end, entries, expect, next_entry, open,
    start_name, start_skip, top,
};
use crate::engine::bit;
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static MAP: DecOps = DecOps {
    value,
    entry,
    named,
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
    match header.major {
        Major::Map => open(state, frames, Phase::Key, entries(header)?),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    match next_entry(state, frames, input)? {
        Entry::Wait => Ok(Flow::NeedInput),
        Entry::End => end(state, frames),
        Entry::Item(header) => match (header.major, header.indefinite()) {
            (Major::Text, false) => Ok(start_name(
                state,
                desc(top(state.depth, frames)?)
                    .layout()
                    .ok_or(ErrorKind::Unsupported)?,
                header.arg,
            )),
            (Major::Text, true) => Err(ErrorKind::Unsupported),
            _ => start_skip(state, 2, Some(header)),
        },
    }
}

fn named(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    matched: Option<u8>,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    match matched {
        None => start_skip(state, 1, None),
        Some(field) if frame.seen & bit(field) != 0 => Err(ErrorKind::DuplicateKey),
        Some(field) => {
            frame.field = field;
            Ok(expect(
                state,
                kinds
                    .get(usize::from(field))
                    .ok_or(ErrorKind::Unsupported)?,
                at(frame.base, layout, field),
            ))
        }
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "an enum's variant lookup can fail"
)]
fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    frame.seen |= bit(frame.field);
    Ok(After::Entry)
}

/// Writes the default of every field the map did not contain.
fn close(frame: &mut Frame) -> Result<(), ErrorKind> {
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    (0..kinds.len())
        .filter_map(|f| u8::try_from(f).ok())
        .try_for_each(|field| {
            if frame.seen & bit(field) != 0 {
                return Ok(());
            }
            frame.field = field;
            let write = kinds
                .get(usize::from(field))
                .and_then(|kind| kind.default_fn())
                .ok_or(ErrorKind::MissingField)?;
            unsafe { write(at(frame.base, layout, field)) };
            frame.seen |= bit(field);
            Ok(())
        })
}

/// Drops exactly the fields the map initialized.
#[inline(never)]
fn drop(frame: &Frame) {
    if let DecKind::Struct { layout, kinds, .. } = desc(frame) {
        (0..kinds.len())
            .filter_map(|f| u8::try_from(f).ok())
            .filter(|&field| frame.seen & bit(field) != 0)
            .for_each(|field| {
                if let Some(drop) = kinds
                    .get(usize::from(field))
                    .and_then(|kind| kind.drop_fn())
                {
                    unsafe { drop(at(frame.base, layout, field)) };
                }
            });
    }
}
