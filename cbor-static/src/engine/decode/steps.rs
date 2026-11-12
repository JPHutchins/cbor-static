//! The steps shared by forms whose entries are named by text or may be skipped: they link the key
//! matcher, UTF-8 validation and the iterative skip only into firmware whose forms start them.

use core::mem::MaybeUninit;

use super::{Flow, Scratch, State, desc, ops, top};
use crate::engine::input::Input;
use crate::engine::skip::Skip;
use crate::engine::text::KeyMatch;
use crate::error::ErrorKind;
use crate::schema::Layout;
use crate::stack::Frame;
use crate::wire::Header;

/// Continues matching a text key or name, then hands the match to the top frame's form.
pub(super) fn name(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let kind = desc(top(state.depth, frames)?);
    let layout = kind.layout().ok_or(ErrorKind::Unsupported)?;
    let key = unsafe { &mut state.scratch.assume_init_mut().key };
    let wanted = key.left();
    let segment = input.take(wanted);
    if cfg!(cbor_static_oneshot) && u64::try_from(segment.len()).unwrap_or(u64::MAX) < wanted {
        return Err(ErrorKind::UnexpectedEnd);
    }
    key.feed(layout, segment)?;
    if key.left() > 0 {
        return Ok(Flow::NeedInput);
    }
    let matched = key.finish()?;
    (ops(kind)?.named)(state, frames, matched)
}

/// Skips the rest of an unwanted entry, then returns to the top frame's next entry.
fn skip(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let skipping = unsafe { &mut state.scratch.assume_init_mut().skip };
    loop {
        let wanted = skipping.payload_left();
        let segment = input.take(wanted);
        if cfg!(cbor_static_oneshot) && u64::try_from(segment.len()).unwrap_or(u64::MAX) < wanted {
            return Err(ErrorKind::UnexpectedEnd);
        }
        skipping.payload(segment)?;
        if skipping.payload_left() > 0 {
            return Ok(Flow::NeedInput);
        }
        if skipping.done() {
            state.step = ops(desc(top(state.depth, frames)?))?.entry;
            return Ok(Flow::Continue);
        }
        let Some(header) = state.carry.header(input, state.strict)? else {
            return Ok(Flow::NeedInput);
        };
        skipping.on_header(header)?;
    }
}

/// Matches a text item of `len` bytes against `layout`'s names, chunk by chunk.
pub(super) fn start_name(state: &mut State, layout: &Layout, len: u64) -> Flow {
    state.scratch.write(Scratch {
        key: KeyMatch::start(layout, len),
    });
    state.step = name;
    Flow::Continue
}

/// Skips the rest of an entry no field claims, which `Strict` admits only as an internally tagged
/// map's tag.
pub(super) fn skip_unknown(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    items: u64,
    header: Option<Header>,
) -> Result<Flow, ErrorKind> {
    if let Some(strict) = state.strict {
        (strict.unknown)(frames, state.depth)?;
    }
    start_skip(state, items, header)
}

/// Skips `items` whole items, the first of which may already have begun with `header`.
pub(super) fn start_skip(
    state: &mut State,
    items: u64,
    header: Option<Header>,
) -> Result<Flow, ErrorKind> {
    let skipping = unsafe {
        &mut state
            .scratch
            .write(Scratch {
                skip: Skip::started(items),
            })
            .skip
    };
    if let Some(header) = header {
        skipping.on_header(header)?;
    }
    state.step = skip;
    Ok(Flow::Continue)
}
