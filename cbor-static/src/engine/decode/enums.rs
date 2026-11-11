//! Externally tagged enums: a unit variant is its name, any other `{name: payload}`.

use core::mem::MaybeUninit;

use super::{
    After, DecOps, Entry, Flow, State, at, desc, end, entries, expect, next_entry, open,
    start_name, top,
};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::{DecKind, DecVariant};
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static EXTERNAL: DecOps = DecOps {
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
    match (header.major, header.indefinite()) {
        (Major::Text, false) => {
            open(state, frames, Phase::EnumUnit, 0)?;
            Ok(start_name(
                state,
                state.target.kind.layout().ok_or(ErrorKind::Unsupported)?,
                header.arg,
            ))
        }
        (Major::Map, indefinite) if indefinite || header.arg == 1 => {
            open(state, frames, Phase::EnumName, entries(header)?)
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let (phase, layout) = (
        frame.phase,
        desc(frame).layout().ok_or(ErrorKind::Unsupported)?,
    );
    match (next_entry(state, frames, input)?, phase) {
        (Entry::Wait, _) => Ok(Flow::NeedInput),
        (Entry::End, _) => end(state, frames),
        (Entry::Item(header), Phase::EnumName)
            if header.major == Major::Text && !header.indefinite() =>
        {
            Ok(start_name(state, layout, header.arg))
        }
        (Entry::Item(_), _) => Err(ErrorKind::TypeMismatch),
    }
}

fn named(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    matched: Option<u8>,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Enum {
        layout, variants, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = matched.ok_or(ErrorKind::UnknownVariant)?;
    match (frame.phase, variants.get(usize::from(variant))) {
        (
            Phase::EnumUnit,
            Some(DecVariant {
                payload: None,
                assemble,
            }),
        ) => {
            unsafe { assemble(frame.base) };
            frame.phase = Phase::EnumDone;
            end(state, frames)
        }
        (
            Phase::EnumName,
            Some(DecVariant {
                payload: Some(kind),
                ..
            }),
        ) => {
            frame.field = variant;
            frame.phase = Phase::EnumPayload;
            Ok(expect(state, kind, at(frame.base, layout, variant)))
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
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
    Ok(After::Entry)
}

fn close(frame: &mut Frame) -> Result<(), ErrorKind> {
    match frame.phase {
        Phase::EnumDone => Ok(()),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn drop(frame: &Frame) {
    if let (
        DecKind::Enum {
            drop: Some(drop), ..
        },
        Phase::EnumDone,
    ) = (desc(frame), frame.phase)
    {
        unsafe { drop(frame.base) };
    }
}
