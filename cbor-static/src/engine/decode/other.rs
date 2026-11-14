//! A map-form struct's `other` field (R-128, #14 Q35), which takes the entries of its map that no
//! field claims: it counts each, then skips it chunk by chunk, or captures it whole from the chunk
//! that holds it, key and value borrowed as they arrived (D-16). `Strict` rejects each first.

use core::mem::MaybeUninit;

use super::map::{heads, select, start_text_key};
use super::{Flow, Item, Next, State, at, desc, ops, start_skip, top};
use crate::engine::bit;
use crate::engine::input::Input;
use crate::engine::skip::{item_len, rest_len};
use crate::engine::text::KeyMatch;
use crate::error::ErrorKind;
use crate::keys::other_field;
use crate::schema::DecKind;
use crate::stack::Frame;
use crate::wire::{Header, Major};

/// A text key, matched whole when this chunk holds its payload, so that an unknown entry can be
/// captured from its head; otherwise matched across chunks as any map matches it.
pub(super) fn text_key<const INT: bool>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let layout = desc(top(state.depth, frames)?)
        .layout()
        .ok_or(ErrorKind::Unsupported)?;
    let Some(payload) = usize::try_from(header.arg)
        .ok()
        .and_then(|len| input.rest().get(..len))
    else {
        return Ok(start_text_key::<INT>(state, layout, header.arg));
    };
    let mut key = if INT {
        KeyMatch::start(layout, header.arg).excluding(heads(layout))
    } else {
        KeyMatch::start(layout, header.arg)
    };
    key.feed(layout, payload)?;
    match key.finish()? {
        Some(field) => {
            input
                .take_exact(payload.len())
                .ok_or(ErrorKind::Unsupported)?;
            select(state, frames, field)
        }
        None => unclaimed(state, frames, input, header, (2, Some(header))),
    }
}

/// The unknown entry whose key's head, `key`, was just read: counted, then skipped as `skip`'s
/// (items, header) say, or captured whole.
pub(super) fn unclaimed(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    key: Header,
    (items, header): (u64, Option<Header>),
) -> Result<Flow, ErrorKind> {
    match claim(state, frames, key, whole(input, key)?.unwrap_or(&[]))? {
        Next::Took(len) => {
            input
                .take_exact(len - key.head_len()?)
                .ok_or(ErrorKind::Unsupported)?;
            state.step = ops(desc(top(state.depth, frames)?))?.entry;
            Ok(Flow::Continue)
        }
        Next::Done => start_skip(state, items, header),
        Next::Then(_) => Err(ErrorKind::Unsupported),
    }
}

/// An unknown text key matched across chunks, so already behind the cursor: counted, then its
/// value skipped; a capture, which needs the whole entry, cannot take it.
pub(super) fn unclaimed_name(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
) -> Result<Flow, ErrorKind> {
    let behind = Header {
        major: Major::Text,
        info: 0,
        arg: 0,
    };
    match claim(state, frames, behind, &[])? {
        Next::Done => start_skip(state, 1, None),
        Next::Took(_) | Next::Then(_) => Err(ErrorKind::Unsupported),
    }
}

/// Hands an unknown entry, `entry` when this chunk holds it whole, to the top frame's `other`
/// field, which its default starts empty.
fn claim(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    key: Header,
    entry: &[u8],
) -> Result<Next, ErrorKind> {
    if let Some(strict) = state.strict {
        (strict.unknown)(frames, state.depth)?;
    }
    let frame = top(state.depth, frames)?;
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    let field = other_field(layout).ok_or(ErrorKind::Unsupported)?;
    let Some(DecKind::Leaf {
        decode, default, ..
    }) = kinds.get(usize::from(field))
    else {
        return Err(ErrorKind::Unsupported);
    };
    let dst = at(frame.base, layout, field);
    if frame.seen & bit(field) == 0 {
        unsafe { default.ok_or(ErrorKind::Unsupported)?(dst) };
        frame.seen |= bit(field);
    }
    unsafe {
        decode(
            dst,
            &Item {
                header: key,
                payload: entry,
                strict: state.strict,
            },
        )
    }
}

/// The whole entry whose key's head, `key`, ends at the cursor, when this chunk holds it.
fn whole<'a>(input: &Input<'a>, key: Header) -> Result<Option<&'a [u8]>, ErrorKind> {
    let head = key.head_len()?;
    let Some(start) = input.at.checked_sub(head) else {
        return Ok(None);
    };
    let rest = input.rest();
    match rest_len(key, rest)
        .and_then(|key_rest| Ok(key_rest + item_len(rest.get(key_rest..).unwrap_or(&[]))?))
    {
        Ok(len) => Ok(input.span(start, head + len)),
        Err(ErrorKind::NeedContiguous) => Ok(None),
        Err(error) => Err(error),
    }
}
