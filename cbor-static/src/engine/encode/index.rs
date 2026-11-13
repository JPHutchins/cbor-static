//! Integer-indexed enums (`#[cbor(index)]`, D-10a): a unit variant is its discriminant alone, any
//! other `[discriminant, payload]`. The layout holds each discriminant pre-encoded.

use core::mem::MaybeUninit;

use super::enums::{named, next};
use super::{Child, EncOps, State};
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::Frame;
use crate::wire::Major;

pub static INDEX: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    named(state, frames, kind, at, Major::Array, 2)
}
