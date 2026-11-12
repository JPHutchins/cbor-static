//! A value wrapped in a CBOR tag: the tag's head, then the value itself. It opens no frame.

use core::mem::MaybeUninit;

use super::option::frameless;
use super::{Child, EncOps, State};
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::Frame;
use crate::wire::Major;

pub static TAG: EncOps = EncOps {
    open,
    next: frameless,
};

fn open(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Tagged { inner, tag, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    state.pending.head(Major::Tag, **tag);
    Ok(Some(Child { kind: inner, at }))
}
