//! Integer-indexed enums (`#[cbor(index)]`, D-10a): a unit variant is its discriminant alone, any
//! other `[discriminant, payload]`. The initial byte tells the two apart, and the layout holds each
//! discriminant pre-encoded, matched in any encoding.

use core::mem::MaybeUninit;

use super::array::{Element, element};
use super::enums::{child_done, close, drop};
use super::map::item_index;
use super::{
    Begin, DecOps, Entry, Flow, State, at, complete, desc, end, entries, expect, next_entry, open,
    top, unnamed,
};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::{DecKind, DecVariant, Layout};
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
        Major::Uint | Major::Nint => unit(state, frames, header),
        Major::Array => open(state, frames, Phase::EnumName, entries(header)?),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

/// A bare discriminant, which only a unit variant is.
fn unit(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Enum {
        layout, variants, ..
    } = state.target.kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    match variant(layout, variants, header)? {
        (
            _,
            DecVariant {
                payload: None,
                assemble,
            },
        ) => {
            unsafe { assemble(state.target.dst) };
            complete(state, frames).map(|()| Flow::Continue)
        }
        (
            _,
            DecVariant {
                payload: Some(_), ..
            },
        ) => Err(ErrorKind::TypeMismatch),
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
            Entry::Item(header) if matches!(header.major, Major::Uint | Major::Nint) => {
                select(state, frames, header)
            }
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

/// Takes the discriminant that opens the array, which only a variant with a payload has.
fn select(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    header: Header,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Enum {
        layout, variants, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    match variant(layout, variants, header)? {
        (_, DecVariant { payload: None, .. }) => Err(ErrorKind::TypeMismatch),
        (
            index,
            DecVariant {
                payload: Some(_), ..
            },
        ) => {
            frame.field = index;
            frame.phase = Phase::EnumPayload;
            Ok(Flow::Continue)
        }
    }
}

/// The variant whose discriminant `header` holds, and its position.
fn variant(
    layout: &Layout,
    variants: &'static [DecVariant],
    header: Header,
) -> Result<(u8, &'static DecVariant), ErrorKind> {
    item_index(layout, header)
        .and_then(|index| Some((index, variants.get(usize::from(index))?)))
        .ok_or(ErrorKind::UnknownVariant)
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
