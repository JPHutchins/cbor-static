//! [`Chunks`]: one definite string of their total length. A definite string's payload is the head's
//! tail; an indefinite one's chunks follow, each a part, from a frame whose `remaining` is the
//! offset of the next one.
#![expect(
    clippy::cast_ptr_alignment,
    reason = "a value pointer comes from a reference or a descriptor offset, so it is aligned"
)]

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, opened};
use crate::engine::chunks::sealed::Chunk;
use crate::engine::chunks::{Chunks, Span};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};

pub static TEXT: EncOps = EncOps {
    open: open::<str>,
    next: next::<str>,
};
pub static BYTES: EncOps = EncOps {
    open: open::<[u8]>,
    next: next::<[u8]>,
};

fn open<T: ?Sized + Chunk>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let span = unsafe { at.cast::<Chunks<'_, T>>().read() }.span;
    state.pending.head(
        T::MAJOR,
        u64::try_from(span.chunks::<[u8]>().map(<[u8]>::len).sum::<usize>()).unwrap_or(u64::MAX),
    );
    match span {
        Span::Definite(payload) => {
            state.pending.tail(payload);
            Ok(None)
        }
        Span::Indefinite(_) => {
            push(frames, &mut state.depth, opened(kind, at, 0, Phase::Value)).map(|()| None)
        }
    }
}

fn next<T: ?Sized + Chunk>(out: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let Span::Indefinite(chunks) = unsafe { frame.base.cast::<Chunks<'_, T>>().read() }.span else {
        return Err(ErrorKind::Unsupported);
    };
    Ok(
        match Span::Indefinite(
            chunks
                .get(usize::try_from(frame.remaining).unwrap_or(usize::MAX)..)
                .unwrap_or(&[]),
        )
        .split_first()
        {
            Some((chunk, rest)) => {
                out.tail(chunk);
                frame.remaining = u64::try_from(chunks.len() - rest.unread()).unwrap_or(u64::MAX);
                Part::Wrote
            }
            None => Part::Finished,
        },
    )
}
