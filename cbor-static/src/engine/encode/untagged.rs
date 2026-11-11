//! Restricted untagged enums (D-10): a unit variant is null, any other is its payload alone.

use core::mem::MaybeUninit;

use super::enums::payload_at;
use super::option::frameless;
use super::{Child, EncOps, State};
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::Frame;
use crate::wire::{Major, NULL};

pub static UNTAGGED: EncOps = EncOps {
    open,
    next: frameless,
};

fn open(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        layout,
        payloads,
        index,
        field_ptr,
        ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = unsafe { index(at) };
    match payloads.get(usize::from(variant)) {
        Some(None) => {
            state.pending.head(Major::Simple, NULL.into());
            Ok(None)
        }
        Some(Some(payload)) => Ok(Some(Child {
            kind: payload,
            at: payload_at(layout, *field_ptr, payload, at, variant),
        })),
        None => Err(ErrorKind::UnknownVariant),
    }
}
