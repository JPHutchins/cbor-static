//! Integer-indexed enums (`#[cbor(index)]`, D-10): `[index]` for a unit variant, otherwise
//! `[index, payload]`. Provisional: unit variants carry no payload element.

use core::mem::MaybeUninit;

use super::array::{Element, element};
use super::enums::{child_done, close, drop};
use super::{
    Begin, DecOps, Entry, Flow, State, at, desc, end, entries, expect, next_entry, open, top,
    unnamed,
};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::{DecKind, DecVariant};
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static INDEX: DecOps = DecOps {
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
    match header.major {
        Major::Array => open(state, frames, Phase::EnumName, entries(header)?),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    match top(state.depth, frames)?.phase {
        Phase::EnumName => match next_entry(state, frames, input)? {
            Entry::Wait => Ok(Flow::NeedInput),
            Entry::Item(Header {
                major: Major::Uint,
                arg,
                ..
            }) => select(
                state,
                frames,
                u8::try_from(arg).map_err(|_| ErrorKind::UnknownVariant)?,
            ),
            Entry::Item(_) | Entry::End => Err(ErrorKind::TypeMismatch),
        },
        Phase::EnumPayload => match element(state, frames, input)? {
            Element::Wait => Ok(Flow::NeedInput),
            Element::Next => payload(state, frames),
            Element::End => Err(ErrorKind::TypeMismatch),
        },
        _ => match element(state, frames, input)? {
            Element::Wait => Ok(Flow::NeedInput),
            Element::End => end(state, frames),
            Element::Next => Err(ErrorKind::TypeMismatch),
        },
    }
}

/// Takes the index: a unit variant is complete, any other expects its payload next.
fn select(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    variant: u8,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Enum { variants, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    frame.field = variant;
    match variants.get(usize::from(variant)) {
        Some(DecVariant {
            payload: None,
            assemble,
        }) => {
            unsafe { assemble(frame.base) };
            frame.phase = Phase::EnumDone;
        }
        Some(DecVariant {
            payload: Some(_), ..
        }) => frame.phase = Phase::EnumPayload,
        None => return Err(ErrorKind::UnknownVariant),
    }
    Ok(Flow::Continue)
}

fn payload(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Enum {
        layout, variants, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    Ok(expect(
        state,
        variants
            .get(usize::from(frame.field))
            .and_then(|variant| variant.payload)
            .ok_or(ErrorKind::Unsupported)?,
        at(frame.base, layout, frame.field),
    ))
}
