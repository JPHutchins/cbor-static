//! Counting a frame's container entries against its length or its break.

use core::mem::MaybeUninit;

use super::{Flow, State, top};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::stack::{Frame, INDEFINITE};
use crate::wire::Header;

/// The next item of a frame's container: a header, the container's end, or not yet available.
pub(super) enum Entry {
    Item(Header),
    End,
    Wait,
}

/// The top frame's next item, counted against its container's length.
pub(super) fn next_entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Entry, ErrorKind> {
    if top(state.depth, frames)?.remaining == 0 {
        return Ok(Entry::End);
    }
    let Some(header) = state.carry.header(input)? else {
        return Ok(Entry::Wait);
    };
    let frame = top(state.depth, frames)?;
    match (frame.remaining, header.is_break()) {
        (INDEFINITE, true) => Ok(Entry::End),
        (INDEFINITE, false) => Ok(Entry::Item(header)),
        (_, true) => Err(ErrorKind::NotWellFormed),
        (_, false) => {
            frame.remaining -= 1;
            Ok(Entry::Item(header))
        }
    }
}

/// `named` for forms whose entries have no names.
pub(super) fn unnamed(
    _: &mut State,
    _: &mut [MaybeUninit<Frame>],
    _: Option<u8>,
) -> Result<Flow, ErrorKind> {
    Err(ErrorKind::Unsupported)
}

pub(super) fn entries(header: Header) -> Result<u64, ErrorKind> {
    match (header.indefinite(), header.arg) {
        (true, _) => Ok(INDEFINITE),
        (false, INDEFINITE) => Err(ErrorKind::LimitExceeded),
        (false, arg) => Ok(arg),
    }
}
