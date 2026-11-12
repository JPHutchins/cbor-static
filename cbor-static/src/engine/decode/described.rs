//! A value the self-describe tag 55799 may precede (RFC 8949 §3.4.6). The form reads one whole head:
//! the tag, which it drops, or the value's own head, which it gives back so the value reads it
//! again. A tag between 256 and 65535 starts with the same byte as 55799, so the initial byte alone
//! cannot tell them apart.

use core::mem::MaybeUninit;

use super::option::{done, holds_nothing, payload_only};
use super::owned::never_a_parent;
use super::{Begin, DecOps, Flow, State, expect, unnamed};
use crate::engine::input::Input;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::Frame;
use crate::wire::{Header, Major, SELF_DESCRIBE, header_len};

pub static SELF_DESCRIBED: DecOps = DecOps {
    value: Begin::Header(value),
    entry: payload_only,
    named: unnamed,
    child_done: never_a_parent,
    close: done,
    drop: holds_nothing,
};

fn value(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Described { inner, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    if (header.major, header.arg) != (Major::Tag, SELF_DESCRIBE) {
        let len = header_len((header.major as u8) << 5 | header.info)?;
        match input.at.checked_sub(len) {
            Some(start) => input.at = start,
            None => state.carry.restore(len),
        }
    }
    Ok(expect(state, inner, state.target.dst))
}
