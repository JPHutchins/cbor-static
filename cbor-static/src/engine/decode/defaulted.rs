//! A field that `default` or `default = "path"` fills when its container lacks it (D-08, #13 Q31):
//! its type's own kind, behind one that only adds the default, so the field decodes as its type
//! does and a type without such fields links none of this.

use core::mem::MaybeUninit;

use super::option::{done, holds_nothing, payload_only};
use super::owned::never_a_parent;
use super::{Begin, DecOps, Flow, State, expect, unnamed};
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::Frame;

pub static DEFAULTED: DecOps = DecOps {
    value: Begin::Peek(value),
    entry: payload_only,
    named: unnamed,
    child_done: never_a_parent,
    close: done,
    drop: holds_nothing,
};

fn value(state: &mut State, _: &mut [MaybeUninit<Frame>], _: u8) -> Result<Flow, ErrorKind> {
    let DecKind::Option { some, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    Ok(expect(state, some, state.target.dst))
}
