//! Items taken whole from the current chunk, which must hold them (D-20): the form reads the head,
//! walks the rest of the item to check it is well-formed, and stores it through its kind. It opens
//! no frame.

use core::mem::MaybeUninit;

use super::option::{done, holds_nothing, payload_only};
use super::owned::never_a_parent;
use super::{Begin, DecOps, Flow, Item, State, complete, unnamed};
use crate::engine::input::Input;
use crate::engine::skip::rest_len;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::Frame;
use crate::wire::{Header, header_len};

/// Checks and writes the value of the whole item, whose `payload` is its every byte, head included.
pub type StoreFn = unsafe fn(*mut u8, &Item<'_>) -> Result<(), ErrorKind>;

pub static RAW: DecOps = DecOps {
    value: Begin::Header(value),
    entry: payload_only,
    named: unnamed,
    child_done: never_a_parent,
    close: done,
    drop: holds_nothing,
};

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Raw { store, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    let head = header_len((header.major as u8) << 5 | header.info)?;
    let start = input
        .at
        .checked_sub(head)
        .ok_or(ErrorKind::NeedContiguous)?;
    let rest = rest_len(header, input.rest())?;
    unsafe {
        store(
            state.target.dst,
            &Item {
                header,
                payload: input
                    .span(start, head + rest)
                    .ok_or(ErrorKind::Unsupported)?,
                strict: state.strict,
            },
        )
    }?;
    input.take_exact(rest).ok_or(ErrorKind::Unsupported)?;
    complete(state, frames).map(|()| Flow::Continue)
}
