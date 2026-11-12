//! The encode core: leaves, output chunking and the frame stack. Every structural form is reached
//! only through the [`EncOps`] in its descriptor, so a firmware links only the forms it uses (D-22).

mod array;
mod cbor_tag;
mod chunks;
mod elements;
mod enums;
mod index;
mod map;
mod option;
mod tagged;
mod untagged;

use core::mem::MaybeUninit;
use core::ptr;

pub use array::ARRAY;
pub use cbor_tag::CBOR_TAG;
pub use chunks::{BYTES as BYTE_CHUNKS, TEXT as TEXT_CHUNKS};
pub use elements::ELEMENTS;
pub use enums::EXTERNAL;
pub use index::INDEX;
pub use map::MAP;
pub use option::OPTION;
pub use tagged::{ADJACENT, INTERNAL};
pub use untagged::UNTAGGED;

use super::pending::Pending;
use super::{Progress, frame_mut, top_field};
use crate::engine::FieldPtrFn;
use crate::error::{Error, ErrorKind};
use crate::schema::{EncKind, Layout};
use crate::stack::{Frame, Phase};

type OnOpen = fn(
    &mut State,
    &mut [MaybeUninit<Frame>],
    &'static EncKind,
    *const u8,
) -> Result<Option<Child>, ErrorKind>;

/// A frame's `field` before it reaches field 0.
const BEFORE_FIRST: u8 = u8::MAX;

/// One structural form's encode handlers.
pub struct EncOps {
    /// Begins a value of this form: writes its heads, then opens a frame or names the value to
    /// encode in its place.
    open: OnOpen,
    /// Produces the next part of the top frame's value.
    next: fn(&mut Pending, &mut Frame) -> Result<Part, ErrorKind>,
}

/// A value to encode next, and where it lives.
struct Child {
    kind: &'static EncKind,
    at: *const u8,
}

enum Part {
    Wrote,
    Child(Child),
    Finished,
}

pub struct State {
    root: &'static EncKind,
    value: *const u8,
    depth: u8,
    started: bool,
    pending: Pending,
    offset: usize,
}

impl State {
    #[inline(never)]
    pub(crate) const fn new(root: &'static EncKind, value: *const u8) -> Self {
        Self {
            root,
            value,
            depth: 0,
            started: false,
            pending: Pending::new(),
            offset: 0,
        }
    }
}

#[inline(never)]
pub fn resume(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    out: &mut [u8],
) -> Result<Progress, Error> {
    let mut written = 0;
    loop {
        let drained = state
            .pending
            .drain(out.get_mut(written..).unwrap_or(&mut []));
        written += drained;
        state.offset += drained;
        if !state.pending.is_empty() {
            return Ok(Progress::NeedSpace(written));
        }
        if !advance(state, frames)
            .map_err(|kind| Error::new(kind, top_field(frames, state.depth), state.offset))?
        {
            return Ok(Progress::Done(written));
        }
    }
}

fn advance(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<bool, ErrorKind> {
    if !state.started {
        state.started = true;
        return emit(state, frames, state.root, state.value).map(|()| true);
    }
    while let Some(top) = state.depth.checked_sub(1) {
        let frame = frame_mut(frames, top)?;
        match (ops(desc(frame))?.next)(&mut state.pending, frame)? {
            Part::Wrote => return Ok(true),
            Part::Child(Child { kind, at }) => return emit(state, frames, kind, at).map(|()| true),
            Part::Finished => state.depth = top,
        }
    }
    Ok(false)
}

fn emit(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<(), ErrorKind> {
    let mut next = Child { kind, at };
    loop {
        match next.kind {
            EncKind::Leaf { encode, .. } => {
                unsafe { encode(next.at, &mut state.pending) };
                return Ok(());
            }
            EncKind::Struct { ops, .. }
            | EncKind::Enum { ops, .. }
            | EncKind::Array { ops, .. }
            | EncKind::Option { ops, .. }
            | EncKind::Chunks { ops } => match (ops.open)(state, frames, next.kind, next.at)? {
                Some(child) => next = child,
                None => return Ok(()),
            },
        }
    }
}

/// A frame for `kind`'s value at `at`, positioned at `field` in `phase`.
fn opened(kind: &'static EncKind, at: *const u8, field: u8, phase: Phase) -> Frame {
    Frame {
        remaining: 0,
        seen: 0,
        desc: ptr::from_ref(kind).cast(),
        base: at.cast_mut(),
        field,
        phase,
    }
}

fn present(
    layout: &Layout,
    kinds: &[&EncKind],
    field_ptr: Option<FieldPtrFn>,
    base: *const u8,
    field: u8,
) -> bool {
    match (
        layout.fields.get(usize::from(field)),
        kinds.get(usize::from(field)),
    ) {
        (
            Some(place),
            Some(
                EncKind::Leaf {
                    absent: Some(absent),
                    ..
                }
                | EncKind::Option {
                    absent: Some(absent),
                    ..
                },
            ),
        ) if place.omit_if_absent => !unsafe { absent(field_addr(layout, field_ptr, base, field)) },
        _ => true,
    }
}

fn field_addr(
    layout: &Layout,
    field_ptr: Option<FieldPtrFn>,
    base: *const u8,
    field: u8,
) -> *const u8 {
    match field_ptr {
        Some(glue) => unsafe { glue(base, field) },
        None => base.wrapping_add(
            layout
                .fields
                .get(usize::from(field))
                .map_or(0, |place| usize::try_from(place.offset).unwrap_or(0)),
        ),
    }
}

fn desc(frame: &Frame) -> &'static EncKind {
    unsafe { &*frame.desc.cast::<EncKind>() }
}

fn ops(kind: &EncKind) -> Result<&'static EncOps, ErrorKind> {
    match kind {
        EncKind::Struct { ops, .. }
        | EncKind::Enum { ops, .. }
        | EncKind::Array { ops, .. }
        | EncKind::Option { ops, .. }
        | EncKind::Chunks { ops } => Ok(ops),
        EncKind::Leaf { .. } => Err(ErrorKind::Unsupported),
    }
}
