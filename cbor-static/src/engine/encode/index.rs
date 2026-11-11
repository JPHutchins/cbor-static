//! Integer-indexed enums (`#[cbor(index)]`, D-10): `[index]` for a unit variant, otherwise
//! `[index, payload]`.

use core::mem::MaybeUninit;

use super::enums::next;
use super::{Child, EncOps, State, opened};
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static INDEX: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        payloads, index, ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = unsafe { index(at) };
    let has_payload = payloads
        .get(usize::from(variant))
        .ok_or(ErrorKind::UnknownVariant)?
        .is_some();
    state.pending.head(Major::Array, 1 + u64::from(has_payload));
    state.pending.head(Major::Uint, variant.into());
    if has_payload {
        push(
            frames,
            &mut state.depth,
            opened(kind, at, variant, Phase::EnumPayload),
        )?;
    }
    Ok(None)
}
