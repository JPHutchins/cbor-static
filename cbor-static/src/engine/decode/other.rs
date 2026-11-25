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
    key.feed(layout, payload);
    match key.finish() {
        Ok(Some(field)) => {
            input
                .take_exact(payload.len())
                .ok_or(ErrorKind::Unsupported)?;
            select(state, frames, field)
        }
        Ok(None) => unclaimed(state, frames, input, header, (2, Some(header))),
        Err(kind) => input
            .take_exact(payload.len())
            .map_or(Err(ErrorKind::Unsupported), |_| Err(kind)),
    }
}

/// The unknown entry whose key's head, `key`, was just read: once admitted, a capture takes it whole,
/// past its key, when this chunk holds it, then it is passed over; otherwise its field is handed it
/// empty, which a capture refuses, and a count's entry is then skipped as (items, header) say.
pub(super) fn unclaimed(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    key: Header,
    (items, header): (u64, Option<Header>),
) -> Result<Flow, ErrorKind> {
    let field = admit(state, frames)?;
    let entry = if captures(state, frames, field)? {
        whole(input, key)?
    } else {
        None
    };
    let key_rest = entry.map_or(0, |(_, key_rest)| key_rest);
    input.take_exact(key_rest).ok_or(ErrorKind::Unsupported)?;
    match (
        claim(
            state,
            frames,
            field,
            key,
            entry.map_or(&[], |(bytes, _)| bytes),
        )?,
        entry.map(|(bytes, _)| bytes.len()),
    ) {
        (Next::Took(len), _) | (Next::Done, Some(len)) => {
            input
                .take_exact(len - key.head_len()? - key_rest)
                .ok_or(ErrorKind::Unsupported)?;
            state.step = ops(desc(top(state.depth, frames)?))?.entry;
            Ok(Flow::Continue)
        }
        (Next::Done, None) => start_skip(state, items, header),
        (Next::Then(_), _) => Err(ErrorKind::Unsupported),
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
    let field = admit(state, frames)?;
    match claim(state, frames, field, behind, &[])? {
        Next::Done => start_skip(state, 1, None),
        Next::Took(_) | Next::Then(_) => Err(ErrorKind::Unsupported),
    }
}

/// The top frame's `other` field, which an unknown entry goes to unless `Strict` refuses it, and
/// which from then on is the field an error names, however the entry was chunked.
fn admit(state: &State, frames: &mut [MaybeUninit<Frame>]) -> Result<u8, ErrorKind> {
    if let Some(strict) = state.strict {
        (strict.unknown)(frames, state.depth)?;
    }
    let frame = top(state.depth, frames)?;
    frame.field = desc(frame)
        .layout()
        .and_then(other_field)
        .ok_or(ErrorKind::Unsupported)?;
    Ok(frame.field)
}

/// Whether the top frame's `other` `field` captures entries, each from the chunk that holds it whole;
/// else it counts them, and each is skipped past any length, in any chunks, as a map without an
/// `other` field skips one.
fn captures(
    state: &State,
    frames: &mut [MaybeUninit<Frame>],
    field: u8,
) -> Result<bool, ErrorKind> {
    Ok(desc(top(state.depth, frames)?)
        .kinds()
        .get(usize::from(field))
        .is_some_and(|kind| kind.takes_payload()))
}

/// Hands an unknown entry, `entry` when this chunk holds it whole, to the top frame's `other`
/// `field`, which its default starts empty.
fn claim(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    field: u8,
    key: Header,
    entry: &[u8],
) -> Result<Next, ErrorKind> {
    use crate::schema::Payload;

    let frame = top(state.depth, frames)?;
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    let Some(DecKind::Leaf {
        decode,
        default,
        payload,
        ..
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
                payload: match payload {
                    Payload::Taken => entry,
                    Payload::Refused => &[],
                },
                strict: state.strict,
            },
        )
    }
}

/// The whole entry whose key's head, `key`, ends at the cursor, and the length of the rest of its
/// key, when this chunk holds it.
fn whole<'a>(input: &Input<'a>, key: Header) -> Result<Option<(&'a [u8], usize)>, ErrorKind> {
    let head = key.head_len()?;
    let Some(start) = input.at.checked_sub(head) else {
        return Ok(None);
    };
    let rest = input.rest();
    match rest_len(key, rest).and_then(|key_rest| {
        Ok((
            key_rest,
            key_rest + item_len(rest.get(key_rest..).unwrap_or(&[]))?,
        ))
    }) {
        Ok((key_rest, len)) => Ok(input.span(start, head + len).map(|entry| (entry, key_rest))),
        Err(ErrorKind::NeedContiguous) => Ok(None),
        Err(error) => Err(error),
    }
}
