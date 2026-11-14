//! The decode core: header assembly, leaves and the frame stack. Every structural form is reached
//! only through the [`DecOps`] in its descriptor, so a firmware links only the forms it uses (D-22).

mod array;
mod cbor_tag;
mod defaulted;
mod described;
mod elements;
mod entries;
mod enums;
mod index;
#[cfg(feature = "heapless")]
mod linear_map;
mod map;
mod option;
mod other;
mod owned;
mod raw;
#[cfg(feature = "heapless")]
mod seq;
mod steps;
mod tagged;
mod untagged;

use core::mem::MaybeUninit;
use core::ptr;

pub use array::ARRAY;
pub use cbor_tag::CBOR_TAG;
pub use defaulted::DEFAULTED;
pub use described::SELF_DESCRIBED;
pub use elements::ELEMENTS;
pub use enums::EXTERNAL;
pub use index::INDEX;
#[cfg(feature = "heapless")]
pub use linear_map::LINEAR_MAP;
pub use map::{
    MAP, MAP_DENY, MAP_INT, MAP_INT_DENY, MAP_INT_OTHER, MAP_MIXED, MAP_MIXED_DENY,
    MAP_MIXED_OTHER, MAP_OTHER,
};
pub use option::OPTION;
pub use owned::{AppendFn, BYTES as BYTES_OWNED, BeginFn, FinishFn, TEXT as TEXT_OWNED};
pub use raw::{RAW, StoreFn};
#[cfg(feature = "heapless")]
pub use seq::SEQ;
pub(crate) use tagged::claim_tag;
pub use tagged::{ADJACENT, INTERNAL};
pub use untagged::UNTAGGED;

use super::input::{Carry, Input};
use super::skip::Skip;
use super::text::KeyMatch;
use super::{Fed, STRICT, StrictOps, frame_mut, push, top_field};
use crate::error::{Error, ErrorKind};
use crate::schema::{DecKind, Layout};
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major, length};
use entries::{Entry, entries, next_entry, unnamed};
use steps::{name, skip_unknown, start_name, start_skip};

/// A header plus its payload, and the profile decoding it. A definite string's payload is its own;
/// an indefinite string's is the rest of the chunk, from which a leaf that accepts chunks takes
/// them.
pub struct Item<'a> {
    pub header: Header,
    pub payload: &'a [u8],
    pub strict: Option<&'static StrictOps>,
}

/// What a leaf decoder needs next: nothing; the following item decoded as another kind; or, having
/// taken an indefinite string's chunks and break from its payload, that many bytes consumed.
pub enum Next {
    Done,
    Then(&'static DecKind),
    Took(usize),
}

/// The core's continuation: what the next bytes of input mean.
type Step = fn(&mut State, &mut [MaybeUninit<Frame>], &mut Input) -> Result<Flow, ErrorKind>;
type OnHeader =
    fn(&mut State, &mut [MaybeUninit<Frame>], &mut Input, Header) -> Result<Flow, ErrorKind>;
type OnPeek = fn(&mut State, &mut [MaybeUninit<Frame>], u8) -> Result<Flow, ErrorKind>;
type OnName = fn(&mut State, &mut [MaybeUninit<Frame>], Option<u8>) -> Result<Flow, ErrorKind>;

/// One structural form's decode handlers.
pub struct DecOps {
    /// A value of this form begins.
    value: Begin,
    /// The top frame, of this form, takes its next entry or ends.
    entry: Step,
    /// A text key or variant name in the top frame matched this entry, or none.
    named: OnName,
    /// The value of the top frame's current entry is complete.
    child_done: fn(&mut Frame) -> Result<After, ErrorKind>,
    /// The top frame's container ended.
    close: fn(&mut Frame) -> Result<(), ErrorKind>,
    /// Drops what the frame initialized.
    drop: fn(&Frame),
}

/// How a form's value begins: with its whole header, or with a look at its initial byte, after
/// which the form retargets and the header is read for the new target.
enum Begin {
    Header(OnHeader),
    Peek(OnPeek),
}

/// What a frame does once an entry's value is complete.
enum After {
    Entry,
    Close,
}

#[derive(Clone, Copy)]
enum Status {
    Running,
    Done,
    Taken,
    Failed(Error),
}

enum Flow {
    Continue,
    NeedInput,
}

#[derive(Clone, Copy)]
struct Target {
    kind: &'static DecKind,
    dst: *mut u8,
}

pub struct State {
    root: Target,
    target: Target,
    step: Step,
    status: Status,
    depth: u8,
    offset: usize,
    carry: Carry,
    scratch: MaybeUninit<Scratch>,
    strict: Option<&'static StrictOps>,
}

/// The key matcher, the skip, a string being copied, and the bytes left of a claimed tag entry's
/// key, which are never live at the same time.
union Scratch {
    key: KeyMatch,
    skip: Skip,
    copy: owned::Copying,
    tag_key_left: u8,
}

impl State {
    #[inline(never)]
    pub(crate) const fn new(kind: &'static DecKind, dst: *mut u8) -> Self {
        Self::under(kind, dst, None)
    }

    #[inline(never)]
    pub(crate) const fn strict(kind: &'static DecKind, dst: *mut u8) -> Self {
        Self::under(kind, dst, Some(&STRICT))
    }

    pub(crate) const fn under(
        kind: &'static DecKind,
        dst: *mut u8,
        strict: Option<&'static StrictOps>,
    ) -> Self {
        let root = Target { kind, dst };
        Self {
            root,
            target: root,
            step: value,
            status: Status::Running,
            depth: 0,
            offset: 0,
            carry: Carry::new(),
            scratch: MaybeUninit::uninit(),
            strict,
        }
    }
}

#[inline(never)]
pub fn feed(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    chunk: &[u8],
) -> Result<Fed, Error> {
    if let Status::Failed(error) = state.status {
        return Err(error);
    }
    let mut input = Input::new(chunk);
    let outcome = run(state, frames, &mut input);
    state.offset += input.at;
    outcome.map_err(|kind| {
        let error = Error::new(kind, top_field(frames, state.depth), state.offset);
        drop_initialized(state, frames);
        state.status = Status::Failed(error);
        error
    })
}

pub fn take(state: &mut State) -> Result<*mut u8, Error> {
    match state.status {
        Status::Done => {
            state.status = Status::Taken;
            Ok(state.root.dst)
        }
        Status::Failed(error) => Err(error),
        Status::Running | Status::Taken => {
            Err(Error::new(ErrorKind::UnexpectedEnd, 0, state.offset))
        }
    }
}

pub fn abandon(state: &mut State, frames: &mut [MaybeUninit<Frame>]) {
    match state.status {
        Status::Done => {
            if let Some(drop) = state.root.kind.drop_fn() {
                unsafe { drop(state.root.dst) };
            }
        }
        Status::Running => drop_initialized(state, frames),
        Status::Taken | Status::Failed(_) => {}
    }
    state.status = Status::Taken;
}

fn run(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Fed, ErrorKind> {
    loop {
        match state.status {
            Status::Running => {}
            Status::Done => return Ok(Fed::Done(input.at)),
            Status::Taken | Status::Failed(_) => return Err(ErrorKind::Unsupported),
        }
        if let Flow::NeedInput = (state.step)(state, frames, input)? {
            return Ok(Fed::NeedInput);
        }
    }
}

/// Decodes the next item into the current target.
fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    if let Ok(DecOps {
        value: Begin::Peek(peek),
        ..
    }) = ops(state.target.kind)
    {
        return match state.carry.initial(input) {
            Some(initial) => peek(state, frames, initial),
            None => Ok(Flow::NeedInput),
        };
    }
    let Some(header) = state.carry.header(input, state.strict)? else {
        return Ok(Flow::NeedInput);
    };
    if header.is_break() {
        return Err(ErrorKind::NotWellFormed);
    }
    match state.target.kind {
        DecKind::Leaf { decode, .. } => {
            let payload = match (header.major, header.indefinite()) {
                (Major::Bytes | Major::Text, true) => input.rest(),
                (Major::Bytes | Major::Text, false) => input
                    .take_exact(length(header.arg)?)
                    .ok_or(ErrorKind::NeedContiguous)?,
                _ => &[],
            };
            match (
                unsafe {
                    decode(
                        state.target.dst,
                        &Item {
                            header,
                            payload,
                            strict: state.strict,
                        },
                    )
                }?,
                header.indefinite(),
            ) {
                (Next::Done | Next::Then(_), true) => Err(ErrorKind::IndefiniteString),
                (Next::Took(_), false) => Err(ErrorKind::Unsupported),
                (Next::Done, false) => complete(state, frames).map(|()| Flow::Continue),
                (Next::Took(taken), true) => input
                    .take_exact(taken)
                    .ok_or(ErrorKind::Unsupported)
                    .and_then(|_| complete(state, frames))
                    .map(|()| Flow::Continue),
                (Next::Then(next), false) => {
                    state.target.kind = next;
                    Ok(Flow::Continue)
                }
            }
        }
        DecKind::Struct { ops, .. }
        | DecKind::Enum { ops, .. }
        | DecKind::Array { ops, .. }
        | DecKind::Option { ops, .. }
        | DecKind::Owned { ops, .. }
        | DecKind::Seq { ops, .. }
        | DecKind::Raw { ops, .. }
        | DecKind::Described { ops, .. }
        | DecKind::Map { ops, .. } => match ops.value {
            Begin::Header(on_header) => on_header(state, frames, input, header),
            Begin::Peek(_) => Err(ErrorKind::Unsupported),
        },
    }
}

/// Decodes the next item as `kind` into `dst`.
fn expect(state: &mut State, kind: &'static DecKind, dst: *mut u8) -> Flow {
    state.target = Target { kind, dst };
    state.step = value;
    Flow::Continue
}

/// Pushes a frame for the current target's container and continues with its entries.
fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    phase: Phase,
    remaining: u64,
) -> Result<Flow, ErrorKind> {
    let Target { kind, dst } = state.target;
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining,
            seen: 0,
            desc: ptr::from_ref(kind).cast(),
            base: dst,
            field: 0,
            phase,
        },
    )?;
    state.step = ops(kind)?.entry;
    Ok(Flow::Continue)
}

/// Ends the top frame's container, then completes the entry that holds it.
fn end(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    (ops(desc(frame))?.close)(frame)?;
    state.depth -= 1;
    complete(state, frames).map(|()| Flow::Continue)
}

/// Completes the current target's entry in each enclosing frame that closes with it.
fn complete(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<(), ErrorKind> {
    while state.depth > 0 {
        let frame = top(state.depth, frames)?;
        let ops = ops(desc(frame))?;
        match (ops.child_done)(frame)? {
            After::Entry => {
                state.step = ops.entry;
                return Ok(());
            }
            After::Close => {
                (ops.close)(frame)?;
                state.depth -= 1;
            }
        }
    }
    state.status = Status::Done;
    Ok(())
}

#[inline(never)]
fn drop_initialized(state: &mut State, frames: &mut [MaybeUninit<Frame>]) {
    frames
        .iter()
        .take(usize::from(state.depth))
        .rev()
        .map(|slot| unsafe { slot.assume_init_ref() })
        .for_each(|frame| {
            if let Ok(ops) = ops(desc(frame)) {
                (ops.drop)(frame);
            }
        });
    state.depth = 0;
}

fn top(depth: u8, frames: &mut [MaybeUninit<Frame>]) -> Result<&mut Frame, ErrorKind> {
    frame_mut(frames, depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?)
}

fn desc(frame: &Frame) -> &'static DecKind {
    unsafe { &*frame.desc.cast::<DecKind>() }
}

fn ops(kind: &DecKind) -> Result<&'static DecOps, ErrorKind> {
    match kind {
        DecKind::Struct { ops, .. }
        | DecKind::Enum { ops, .. }
        | DecKind::Array { ops, .. }
        | DecKind::Option { ops, .. }
        | DecKind::Owned { ops, .. }
        | DecKind::Seq { ops, .. }
        | DecKind::Raw { ops, .. }
        | DecKind::Described { ops, .. }
        | DecKind::Map { ops, .. } => Ok(ops),
        DecKind::Leaf { .. } => Err(ErrorKind::Unsupported),
    }
}

fn at(base: *mut u8, layout: &Layout, field: u8) -> *mut u8 {
    base.wrapping_add(
        layout
            .fields
            .get(usize::from(field))
            .map_or(0, |place| usize::try_from(place.offset).unwrap_or(0)),
    )
}
