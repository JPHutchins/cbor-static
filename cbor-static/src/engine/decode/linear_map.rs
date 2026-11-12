//! A map of up to a capacity of entries (heapless `LinearMap`). Its entries decode in place into a
//! shadow sequence of `(K, V)` held in the map's own slot, since the map exposes no storage of its
//! own, and the shadow becomes the map when the map closes. The frame's `seen` counts whole
//! entries; its phase says whether entry `seen`'s key or value comes next.

use core::mem::MaybeUninit;

use super::array::{Element, element};
use super::seq::{index, slot};
use super::{After, Begin, DecOps, Flow, State, desc, end, entries, expect, open, top, unnamed};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::{DecKind, MapGlue};
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major};

pub static LINEAR_MAP: DecOps = DecOps {
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
    let DecKind::Map { glue, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    match header.major {
        Major::Map => {
            unsafe { (glue.begin)(state.target.dst) };
            open(state, frames, Phase::Key, entries(header)?)
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

/// A key, counted against the map's length; or, once that key is complete and differs from every
/// earlier one, its value.
fn entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    if top(state.depth, frames)?.phase == Phase::Value {
        let frame = top(state.depth, frames)?;
        let (base, seen) = (frame.base, frame.seen);
        let DecKind::Map { value, glue, .. } = desc(frame) else {
            return Err(ErrorKind::Unsupported);
        };
        let entry = slot(glue.at, base, seen)?;
        distinct(state, frames, glue, base, entry, seen)?;
        return Ok(expect(state, value, entry.wrapping_add(glue.value_at)));
    }
    match element(state, frames, input)? {
        Element::Wait => return Ok(Flow::NeedInput),
        Element::End => return end(state, frames),
        Element::Next => {}
    }
    let frame = top(state.depth, frames)?;
    let DecKind::Map { key, glue, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    Ok(expect(
        state,
        key,
        slot(glue.at, frame.base, frame.seen)?.wrapping_add(glue.key_at),
    ))
}

/// Whether the key of entry `seen`, at `entry`, may join the map: under `Strict`, when its encoding
/// sorts after the previous key's (Q29), which makes it unique too; otherwise, when it equals no
/// earlier key (Q2).
fn distinct(
    state: &State,
    frames: &mut [MaybeUninit<Frame>],
    glue: &MapGlue,
    base: *mut u8,
    entry: *mut u8,
    seen: u64,
) -> Result<(), ErrorKind> {
    match state.strict {
        Some(strict) if seen > 0 => (strict.keys)(
            glue.key_encode,
            entry.wrapping_sub(glue.stride).wrapping_add(glue.key_at),
            entry.wrapping_add(glue.key_at),
            frames
                .get_mut(usize::from(state.depth)..)
                .unwrap_or(&mut []),
        ),
        Some(_) => Ok(()),
        None if unsafe { (glue.fresh)(base, index(seen)) } => Ok(()),
        None => Err(ErrorKind::DuplicateKey),
    }
}

fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    let DecKind::Map { glue, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.phase == Phase::Value {
        frame.seen += 1;
        frame.field = u8::try_from(frame.seen).unwrap_or(u8::MAX);
        unsafe { (glue.at)(frame.base, index(frame.seen)) };
        frame.phase = Phase::Key;
    } else {
        frame.phase = Phase::Value;
    }
    Ok(After::Entry)
}

fn close(frame: &mut Frame) -> Result<(), ErrorKind> {
    let DecKind::Map { glue, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    unsafe { (glue.assemble)(frame.base) };
    Ok(())
}

/// Drops the entries committed so far, and a complete key whose value was still to come.
fn drop(frame: &Frame) {
    let DecKind::Map { glue, .. } = desc(frame) else {
        return;
    };
    if frame.phase == Phase::Value
        && let Some(drop_key) = glue.drop_key
    {
        unsafe { drop_key((glue.at)(frame.base, index(frame.seen)).wrapping_add(glue.key_at)) };
    }
    if let Some(drop_shadow) = glue.drop_shadow {
        unsafe { drop_shadow(frame.base) };
    }
}
