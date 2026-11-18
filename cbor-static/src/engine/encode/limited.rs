//! A value whose length `#[cbor(max_len = N)]` bounds (#5 Q15): its own kind, behind one that
//! checks the length and opens no frame, so a type without such fields links none of this.

use core::mem::MaybeUninit;

use super::option::frameless;
use super::{Child, EncOps, State};
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::Frame;

pub static LIMITED: EncOps = EncOps {
    open,
    next: frameless,
};

fn open(
    _: &mut State,
    _: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Option { some, some_ptr, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    let within = unsafe { some_ptr(at) };
    if within.is_null() {
        return Err(ErrorKind::LimitExceeded);
    }
    Ok(Some(Child {
        kind: some,
        at: within,
    }))
}
