//! Enums discriminated by CBOR tag (D-10): the variant's tag head, then its payload.

use core::mem::MaybeUninit;

use super::enums::next;
use super::{Child, EncOps, State, opened};
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};

pub static CBOR_TAG: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Enum {
        layout,
        payloads,
        index,
        ..
    } = kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    let variant = unsafe { index(at) };
    match payloads.get(usize::from(variant)) {
        Some(Some(_)) => {
            state.pending.tail(layout.name_item(variant));
            push(
                frames,
                &mut state.depth,
                opened(kind, at, variant, Phase::EnumPayload),
            )
            .map(|()| None)
        }
        Some(None) => Err(ErrorKind::Unsupported),
        None => Err(ErrorKind::UnknownVariant),
    }
}
