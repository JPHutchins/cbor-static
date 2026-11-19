//! A value a `#[cbor(with = module)]` field's module encodes (#5 Q19), through a [`TokenWriter`]
//! that keeps only the part of its one item that `Pending` takes next: opening runs the module once
//! to check the item and count it, and each next part runs it again. Only such fields link this.

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use super::stopped::{promoted, stop};
use super::{Child, EncOps, Part, State, opened};
use crate::engine::pending::{CAPACITY, Pending};
use crate::engine::{push, top_field};
use crate::error::{Error, ErrorKind};
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::token::{TokenWriter, Window};

/// A field's module, which writes its value as exactly one item. The derive implements it on a
/// type of its own per field, whose `encode` calls the module's.
pub trait WithEncode {
    type Value;
    /// # Errors
    /// Whatever the module's `encode` returns.
    fn encode(value: &Self::Value, writer: &mut TokenWriter<'_>) -> Result<(), Error>;
}

/// The descriptor of a value `M` encodes.
pub struct WithEncoding<M>(PhantomData<M>);

impl<M: WithEncode> WithEncoding<M> {
    pub const ENCODE: &'static EncKind = &EncKind::Select {
        ops: &EncOps {
            open: open::<M>,
            next: next::<M>,
        },
    };
}

/// A frame's `seen` once a run disagrees with the first: no run's length.
const DISAGREED: u64 = u64::MAX;

/// Checks the value's item and counts it, stopping the encoding where the module fails; the frame
/// keeps the count in `seen` and the bytes written in `remaining`, and names its parent's field.
fn open<M: WithEncode>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    match written::<M>(at, 0, &mut []) {
        Ok(len) => {
            let field = top_field(frames, state.depth);
            push(
                frames,
                &mut state.depth,
                Frame {
                    seen: u64::try_from(len).unwrap_or(DISAGREED),
                    ..opened(kind, at, field, Phase::Value)
                },
            )
            .map(|()| None)
        }
        Err(kind) => Err(stop(state, promoted(kind))),
    }
}

/// The next part of the item, as much as `Pending` holds, written by running the module again.
fn next<M: WithEncode>(out: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    if frame.remaining >= frame.seen {
        return Ok(Part::Finished);
    }
    let mut part = [0u8; CAPACITY];
    let from = usize::try_from(frame.remaining).map_err(|_| ErrorKind::LimitExceeded)?;
    match written::<M>(frame.base, from, &mut part) {
        Ok(len) if u64::try_from(len) == Ok(frame.seen) => {
            let taken = part.len().min(len.saturating_sub(from));
            out.bytes(part.get(..taken).unwrap_or(&[]));
            frame.remaining = u64::try_from(from + taken).unwrap_or(DISAGREED);
            Ok(Part::Wrote)
        }
        Ok(_) | Err(_) => {
            frame.seen = DISAGREED;
            Err(ErrorKind::NotWellFormed)
        }
    }
}

/// Runs the module on the value at `at`, keeping in `part` the bytes it writes from `from` on, and
/// returns the length of the one item it wrote.
fn written<M: WithEncode>(at: *const u8, from: usize, part: &mut [u8]) -> Result<usize, ErrorKind> {
    let mut window = Window::new(from);
    let mut writer = TokenWriter::window(part, &mut window);
    M::encode(unsafe { &*at.cast::<M::Value>() }, &mut writer).map_err(|error| error.kind)?;
    writer.item_len()
}
