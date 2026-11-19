//! [`crate::Embedded`]: a value in tag 24's byte string (#5 Q18). Opening counts the value's
//! encoding with an encoder of its own, under the root's deterministic encoding, then writes the tag;
//! the frame writes the byte string's head, then encodes the value in place. Only `Embedded` links
//! this.

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use super::stopped::{promoted, stop};
use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::{Encoder, push};
use crate::error::ErrorKind;
use crate::schema::{EncKind, Encode};
use crate::sizes::counted;
use crate::stack::{Frame, Phase};
use crate::wire::{ENCODED_CBOR, Major};

/// The descriptor of an embedded `T`.
pub struct EmbeddedEncoding<T>(PhantomData<T>);

impl<T: Encode> EmbeddedEncoding<T> {
    pub const ENCODE: &'static EncKind = &EncKind::Tagged {
        ops: &EncOps {
            open: open::<T>,
            next,
        },
        inner: T::ENCODE,
        tag: &ENCODED_CBOR,
    };
}

/// Writes the tag, and opens a frame whose `remaining` is the value's length and whose `seen`
/// counts the parts written: the head, then the value.
fn open<T: Encode>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    match counted(Encoder::under(
        unsafe { &*at.cast::<T>() },
        state.pending.deterministic(),
    )) {
        Ok(len) => {
            state.pending.head(Major::Tag, ENCODED_CBOR);
            push(
                frames,
                &mut state.depth,
                Frame {
                    remaining: u64::try_from(len).map_err(|_| ErrorKind::LimitExceeded)?,
                    ..opened(kind, at, 0, Phase::Value)
                },
            )
            .map(|()| None)
        }
        Err(error) => Err(stop(state, promoted(error.kind))),
    }
}

fn next(out: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Tagged { inner, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    frame.seen += 1;
    Ok(match frame.seen {
        1 => {
            out.head(Major::Bytes, frame.remaining);
            Part::Wrote
        }
        2 => Part::Child(Child {
            kind: inner,
            at: frame.base.cast_const(),
        }),
        _ => Part::Finished,
    })
}
