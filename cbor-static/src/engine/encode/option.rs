//! `Option<T>` of a nested `T`: null, or `T` itself.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State};
use crate::engine::pending::Pending;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::Frame;
use crate::wire::{Major, NULL};

pub static OPTION: EncOps = EncOps {
    open,
    next: frameless,
};

fn open(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Option { some, some_ptr, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    let payload = unsafe { some_ptr(at) };
    if payload.is_null() {
        state.pending.head(Major::Simple, NULL.into());
        return Ok(None);
    }
    Ok(Some(Child {
        kind: some,
        at: payload,
    }))
}

/// An `Option` opens no frame, so it is never asked for a next part.
#[expect(clippy::unnecessary_wraps, reason = "EncOps::next can fail")]
pub(super) const fn frameless(_: &mut Pending, _: &mut Frame) -> Result<Part, ErrorKind> {
    Ok(Part::Finished)
}
