use core::mem::MaybeUninit;
use core::ptr;

use super::input::{Carry, Input};
use super::skip::Skip;
use super::text::KeyMatch;
use super::{Fed, bit, frame_mut, push};
use crate::error::{Error, ErrorKind};
use crate::schema::{DecKind, DecVariant, Layout};
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

#[derive(Clone, Copy)]
enum Mode {
    Value,
    Key,
    Name,
    Skip,
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
    mode: Mode,
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
            mode: Mode::Value,
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
    if let Mode::Failed(error) = state.mode {
        return Err(error);
    }
    let mut input = Input::new(chunk);
    let outcome = run(state, frames, &mut input);
    state.offset += input.at;
    outcome.map_err(|kind| {
        let field = state
            .depth
            .checked_sub(1)
            .and_then(|top| frame_mut(frames, top).ok())
            .map_or(0, |f| f.field);
        let error = Error::new(kind, field, state.offset);
        drop_initialized(state, frames);
        state.mode = Mode::Failed(error);
        error
    })
}

pub fn take(state: &mut State) -> Result<*mut u8, Error> {
    match state.mode {
        Mode::Done => {
            state.mode = Mode::Taken;
            Ok(state.root.dst)
        }
        Mode::Failed(error) => Err(error),
        _ => Err(Error::new(ErrorKind::UnexpectedEnd, 0, state.offset)),
    }
}

pub fn abandon(state: &mut State, frames: &mut [MaybeUninit<Frame>]) {
    match state.mode {
        Mode::Done => {
            if let Some(drop) = state.root.kind.drop_fn() {
                unsafe { drop(state.root.dst) };
            }
        }
        Mode::Value | Mode::Key | Mode::Name | Mode::Skip => drop_initialized(state, frames),
        Mode::Taken | Mode::Failed(_) => {}
    }
    state.mode = Mode::Taken;
}

fn run(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Fed, ErrorKind> {
    loop {
        let flow = match state.mode {
            Mode::Value => value(state, frames, input)?,
            Mode::Key => key(state, frames, input)?,
            Mode::Name => name(state, frames, input)?,
            Mode::Skip => skip(state, input)?,
            Mode::Done => return Ok(Fed::Done(input.at)),
            Mode::Taken | Mode::Failed(_) => return Err(ErrorKind::Unsupported),
        };
        if let Flow::NeedInput = flow {
            return Ok(Fed::NeedInput);
        }
    }
}

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let Some(header) = state.carry.header(input)? else {
        return Ok(Flow::NeedInput);
    };
    let Target { kind, dst } = state.target;
    match (kind, header.major) {
        _ if header.is_break() => Err(ErrorKind::NotWellFormed),
        (DecKind::Struct { .. }, Major::Map) => open(state, frames, Phase::Key, entries(header)?),
        (DecKind::Enum { layout, .. }, Major::Text) if !header.indefinite() => {
            open(state, frames, Phase::EnumUnit, 0)?;
            state.key.start(layout, header.arg);
            state.mode = Mode::Name;
            Ok(Flow::Continue)
        }
        (DecKind::Enum { .. }, Major::Map) if header.indefinite() || header.arg == 1 => {
            open(state, frames, Phase::EnumName, entries(header)?)
        }
        (DecKind::Leaf { decode, .. }, _) => {
            let payload = match (header.major, header.indefinite()) {
                (Major::Bytes | Major::Text, true) => return Err(ErrorKind::IndefiniteString),
                (Major::Bytes | Major::Text, false) => input
                    .take_exact(header.arg)
                    .ok_or(ErrorKind::NeedContiguous)?,
                _ => &[],
            };
            match unsafe { decode(dst, &Item { header, payload }) }? {
                super::Next::Done => complete(state, frames).map(|()| Flow::Continue),
                super::Next::Then(next) => {
                    state.target.kind = next;
                    Ok(Flow::Continue)
                }
            }
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn key(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let top = state.depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?;
    if frame_mut(frames, top)?.remaining == 0 {
        return close(state, frames);
    }
    let Some(header) = state.carry.header(input)? else {
        return Ok(Flow::NeedInput);
    };
    let frame = frame_mut(frames, top)?;
    if frame.remaining == INDEFINITE {
        if header.is_break() {
            return close(state, frames);
        }
    } else {
        frame.remaining -= 1;
    }
    let kind = desc(frame);
    match (kind, frame.phase, header.major, header.indefinite()) {
        (_, Phase::EnumDone, ..) => Err(ErrorKind::TypeMismatch),
        (_, _, Major::Text, false) => {
            state
                .key
                .start(kind.layout().ok_or(ErrorKind::Unsupported)?, header.arg);
            state.mode = Mode::Name;
            Ok(Flow::Continue)
        }
        (DecKind::Struct { .. }, _, Major::Text, true) => Err(ErrorKind::Unsupported),
        (DecKind::Struct { .. }, ..) => {
            state.skip.start(2);
            state.skip.on_header(header)?;
            state.mode = Mode::Skip;
            Ok(Flow::Continue)
        }
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn name(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let top = state.depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?;
    let frame = frame_mut(frames, top)?;
    let kind = desc(frame);
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
    let base = frame.base;
    match (kind, frame.phase, state.key.finish()?) {
        (DecKind::Struct { .. }, _, None) => {
            state.skip.start(1);
            state.mode = Mode::Skip;
        }
        (DecKind::Struct { kinds, .. }, _, Some(field)) => {
            if frame.seen & bit(field) != 0 {
                return Err(ErrorKind::DuplicateKey);
            }
            frame.field = field;
            frame.phase = Phase::Value;
            let kind = kinds
                .get(usize::from(field))
                .ok_or(ErrorKind::Unsupported)?;
            state.target = Target {
                kind,
                dst: at(base, layout, field),
            };
            state.mode = Mode::Value;
        }
        (DecKind::Enum { variants, .. }, phase, Some(variant)) => {
            match (phase, variants.get(usize::from(variant))) {
                (
                    Phase::EnumUnit,
                    Some(DecVariant {
                        payload: None,
                        assemble,
                    }),
                ) => {
                    unsafe { assemble(base) };
                    frame.phase = Phase::EnumDone;
                    return close(state, frames);
                }
                (
                    Phase::EnumName,
                    Some(DecVariant {
                        payload: Some(kind),
                        ..
                    }),
                ) => {
                    frame.field = variant;
                    frame.phase = Phase::EnumPayload;
                    state.target = Target {
                        kind,
                        dst: at(base, layout, variant),
                    };
                    state.mode = Mode::Value;
                }
                _ => return Err(ErrorKind::TypeMismatch),
            }
        }
        (DecKind::Enum { .. }, _, None) => return Err(ErrorKind::UnknownVariant),
        (DecKind::Leaf { .. }, ..) => return Err(ErrorKind::Unsupported),
    }
    Ok(Flow::Continue)
}

fn skip(state: &mut State, input: &mut Input) -> Result<Flow, ErrorKind> {
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
            state.mode = Mode::Key;
            return Ok(Flow::Continue);
        }
        let Some(header) = state.carry.header(input)? else {
            return Ok(Flow::NeedInput);
        };
        state.skip.on_header(header)?;
    }
}

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
    state.mode = Mode::Key;
    Ok(Flow::Continue)
}

fn close(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<Flow, ErrorKind> {
    let top = state.depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?;
    let frame = frame_mut(frames, top)?;
    match (desc(frame), frame.phase) {
        (DecKind::Struct { layout, kinds, .. }, _) => fill_defaults(frame, layout, kinds)?,
        (DecKind::Enum { .. }, Phase::EnumDone) => {}
        _ => return Err(ErrorKind::TypeMismatch),
    }
    state.depth = top;
    complete(state, frames).map(|()| Flow::Continue)
}

fn complete(state: &mut State, frames: &mut [MaybeUninit<Frame>]) -> Result<(), ErrorKind> {
    let Some(top) = state.depth.checked_sub(1) else {
        state.mode = Mode::Done;
        return Ok(());
    };
    let frame = frame_mut(frames, top)?;
    match (desc(frame), frame.phase) {
        (DecKind::Struct { .. }, Phase::Value) => frame.seen |= bit(frame.field),
        (DecKind::Enum { variants, .. }, Phase::EnumPayload) => {
            let variant = variants
                .get(usize::from(frame.field))
                .ok_or(ErrorKind::Unsupported)?;
            unsafe { (variant.assemble)(frame.base) };
        }
        _ => return Err(ErrorKind::Unsupported),
    }
    frame.phase = match frame.phase {
        Phase::EnumPayload => Phase::EnumDone,
        _ => Phase::Key,
    };
    state.mode = Mode::Key;
    Ok(())
}

fn fill_defaults(frame: &mut Frame, layout: &Layout, kinds: &[&DecKind]) -> Result<(), ErrorKind> {
    (0..kinds.len())
        .filter_map(|f| u8::try_from(f).ok())
        .try_for_each(|field| {
            if frame.seen & bit(field) != 0 {
                return Ok(());
            }
            frame.field = field;
            let write = kinds
                .get(usize::from(field))
                .and_then(|kind| kind.default_fn())
                .ok_or(ErrorKind::MissingField)?;
            unsafe { write(at(frame.base, layout, field)) };
            frame.seen |= bit(field);
            Ok(())
        })
}

#[inline(never)]
fn drop_initialized(state: &mut State, frames: &mut [MaybeUninit<Frame>]) {
    frames
        .iter_mut()
        .take(usize::from(state.depth))
        .rev()
        .for_each(|slot| {
            let frame = unsafe { slot.assume_init_mut() };
            match desc(frame) {
                DecKind::Struct { layout, kinds, .. } => {
                    (0..kinds.len())
                        .filter_map(|f| u8::try_from(f).ok())
                        .for_each(|field| {
                            if let Some(drop) = kinds
                                .get(usize::from(field))
                                .and_then(|kind| kind.drop_fn())
                                .filter(|_| frame.seen & bit(field) != 0)
                            {
                                unsafe { drop(at(frame.base, layout, field)) };
                            }
                        });
                }
                DecKind::Enum {
                    drop: Some(drop), ..
                } if frame.phase == Phase::EnumDone => unsafe { drop(frame.base) },
                DecKind::Enum { .. } | DecKind::Leaf { .. } => {}
            }
        });
    state.depth = 0;
}

fn entries(header: Header) -> Result<u64, ErrorKind> {
    match (header.indefinite(), header.arg) {
        (true, _) => Ok(INDEFINITE),
        (false, INDEFINITE) => Err(ErrorKind::LimitExceeded),
        (false, arg) => Ok(arg),
    }
}

fn desc(frame: &Frame) -> &'static DecKind {
    unsafe { &*frame.desc.cast::<DecKind>() }
}

fn at(base: *mut u8, layout: &Layout, field: u8) -> *mut u8 {
    base.wrapping_add(
        layout
            .fields
            .get(usize::from(field))
            .map_or(0, |place| usize::try_from(place.offset).unwrap_or(0)),
    )
}
