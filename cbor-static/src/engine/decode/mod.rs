//! The decode core: header assembly, leaves and the frame stack. Every structural form is reached
//! only through the [`DecOps`] in its descriptor, so a firmware links only the forms it uses (D-22).

mod enums;
mod map;

use core::mem::MaybeUninit;
use core::ptr;

pub use enums::EXTERNAL;
pub use map::MAP;

use super::input::{Carry, Input};
use super::skip::Skip;
use super::text::KeyMatch;
use super::{Fed, frame_mut, push, top_field};
use crate::error::{Error, ErrorKind};
use crate::schema::{DecKind, Layout};
use crate::stack::{Frame, INDEFINITE, Phase};
use crate::wire::{Header, Major};

/// A header plus, for definite strings, its payload.
pub struct Item<'a> {
    pub header: Header,
    pub payload: &'a [u8],
}

/// What a leaf decoder needs next: nothing, or the following item decoded as another kind.
pub enum Next {
    Done,
    Then(&'static DecKind),
}

/// The core's continuation: what the next bytes of input mean.
type Step = fn(&mut State, &mut [MaybeUninit<Frame>], &mut Input) -> Result<Flow, ErrorKind>;
type OnValue =
    fn(&mut State, &mut [MaybeUninit<Frame>], &mut Input, Header) -> Result<Flow, ErrorKind>;
type OnName = fn(&mut State, &mut [MaybeUninit<Frame>], Option<u8>) -> Result<Flow, ErrorKind>;

/// One structural form's decode handlers.
pub struct DecOps {
    /// A header arrived where a value of this form is expected.
    value: OnValue,
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

/// What a frame does once an entry's value is complete.
enum After {
    Entry,
    #[expect(
        dead_code,
        reason = "frames without a container of their own close with their only entry"
    )]
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

/// The next item of a frame's container: a header, the container's end, or not yet available.
enum Entry {
    Item(Header),
    End,
    Wait,
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
    key: KeyMatch,
    skip: Skip,
}

impl State {
    #[inline(never)]
    pub(crate) const fn new(kind: &'static DecKind, dst: *mut u8) -> Self {
        let root = Target { kind, dst };
        Self {
            root,
            target: root,
            step: value,
            status: Status::Running,
            depth: 0,
            offset: 0,
            carry: Carry::new(),
            key: KeyMatch::new(),
            skip: Skip::new(),
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
    let Some(header) = state.carry.header(input)? else {
        return Ok(Flow::NeedInput);
    };
    if header.is_break() {
        return Err(ErrorKind::NotWellFormed);
    }
    match state.target.kind {
        DecKind::Leaf { decode, .. } => {
            let payload = match (header.major, header.indefinite()) {
                (Major::Bytes | Major::Text, true) => return Err(ErrorKind::IndefiniteString),
                (Major::Bytes | Major::Text, false) => input
                    .take_exact(header.arg)
                    .ok_or(ErrorKind::NeedContiguous)?,
                _ => &[],
            };
            match unsafe { decode(state.target.dst, &Item { header, payload }) }? {
                Next::Done => complete(state, frames).map(|()| Flow::Continue),
                Next::Then(next) => {
                    state.target.kind = next;
                    Ok(Flow::Continue)
                }
            }
        }
        DecKind::Struct { ops, .. } | DecKind::Enum { ops, .. } => {
            (ops.value)(state, frames, input, header)
        }
    }
}

/// Continues matching a text key or name, then hands the match to the top frame's form.
fn name(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let kind = desc(top(state.depth, frames)?);
    let layout = kind.layout().ok_or(ErrorKind::Unsupported)?;
    let wanted = state.key.left();
    let segment = input.take(wanted);
    if cfg!(cbor_static_oneshot) && u64::try_from(segment.len()).unwrap_or(u64::MAX) < wanted {
        return Err(ErrorKind::UnexpectedEnd);
    }
    state.key.feed(layout, segment)?;
    if state.key.left() > 0 {
        return Ok(Flow::NeedInput);
    }
    let matched = state.key.finish()?;
    (ops(kind)?.named)(state, frames, matched)
}

/// Skips the rest of an unwanted entry, then returns to the top frame's next entry.
fn skip(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    loop {
        let wanted = state.skip.payload_left();
        let segment = input.take(wanted);
        if cfg!(cbor_static_oneshot) && u64::try_from(segment.len()).unwrap_or(u64::MAX) < wanted {
            return Err(ErrorKind::UnexpectedEnd);
        }
        state.skip.payload(segment)?;
        if state.skip.payload_left() > 0 {
            return Ok(Flow::NeedInput);
        }
        if state.skip.done() {
            state.step = ops(desc(top(state.depth, frames)?))?.entry;
            return Ok(Flow::Continue);
        }
        let Some(header) = state.carry.header(input)? else {
            return Ok(Flow::NeedInput);
        };
        state.skip.on_header(header)?;
    }
}

/// Matches a text item of `len` bytes against `layout`'s names, chunk by chunk.
fn start_name(state: &mut State, layout: &Layout, len: u64) -> Flow {
    state.key.start(layout, len);
    state.step = name;
    Flow::Continue
}

/// Skips `items` whole items, the first of which may already have begun with `header`.
fn start_skip(state: &mut State, items: u64, header: Option<Header>) -> Result<Flow, ErrorKind> {
    state.skip.start(items);
    if let Some(header) = header {
        state.skip.on_header(header)?;
    }
    state.step = skip;
    Ok(Flow::Continue)
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

/// The top frame's next item, counted against its container's length.
fn next_entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Entry, ErrorKind> {
    if top(state.depth, frames)?.remaining == 0 {
        return Ok(Entry::End);
    }
    let Some(header) = state.carry.header(input)? else {
        return Ok(Entry::Wait);
    };
    let frame = top(state.depth, frames)?;
    match (frame.remaining, header.is_break()) {
        (INDEFINITE, true) => Ok(Entry::End),
        (INDEFINITE, false) => Ok(Entry::Item(header)),
        (_, true) => Err(ErrorKind::NotWellFormed),
        (_, false) => {
            frame.remaining -= 1;
            Ok(Entry::Item(header))
        }
    }
}

fn entries(header: Header) -> Result<u64, ErrorKind> {
    match (header.indefinite(), header.arg) {
        (true, _) => Ok(INDEFINITE),
        (false, INDEFINITE) => Err(ErrorKind::LimitExceeded),
        (false, arg) => Ok(arg),
    }
}

fn top(depth: u8, frames: &mut [MaybeUninit<Frame>]) -> Result<&mut Frame, ErrorKind> {
    frame_mut(frames, depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?)
}

fn desc(frame: &Frame) -> &'static DecKind {
    unsafe { &*frame.desc.cast::<DecKind>() }
}

fn ops(kind: &DecKind) -> Result<&'static DecOps, ErrorKind> {
    match kind {
        DecKind::Struct { ops, .. } | DecKind::Enum { ops, .. } => Ok(ops),
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
