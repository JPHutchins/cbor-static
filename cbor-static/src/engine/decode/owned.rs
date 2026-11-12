//! Strings copied into the value as their payload arrives, so they decode chunk by chunk, from a
//! definite or an indefinite string (D-13, D-16). The form opens no frame: its progress lives in the
//! decoder's scratch, which nothing else holds while a payload is copied.

use core::mem::MaybeUninit;

use super::option::{done, holds_nothing, payload_only};
use super::{After, Begin, DecOps, Flow, Scratch, State, complete, unnamed};
use crate::engine::input::Input;
use crate::engine::text::Utf8;
use crate::error::ErrorKind;
use crate::schema::DecKind;
use crate::stack::Frame;
use crate::wire::{Header, Major, length};

/// Writes the empty value into the target.
pub type BeginFn = unsafe fn(*mut u8);
/// Copies the next segment of the payload, which starts `at` bytes in.
pub type AppendFn = unsafe fn(*mut u8, usize, &[u8]) -> Result<(), ErrorKind>;
/// Checks, and completes, a value of this many bytes.
pub type FinishFn = unsafe fn(*mut u8, usize) -> Result<(), ErrorKind>;

pub static TEXT: DecOps = DecOps {
    value: Begin::Header(text),
    entry: payload_only,
    named: unnamed,
    child_done: never_a_parent,
    close: done,
    drop: holds_nothing,
};

pub static BYTES: DecOps = DecOps {
    value: Begin::Header(bytes),
    ..TEXT
};

/// The string being copied: the payload the current chunk still owes, the bytes copied so far,
/// whether chunks follow until a break, and for text, the UTF-8 state across segments.
#[derive(Clone, Copy)]
pub struct Copying {
    left: u64,
    total: usize,
    chunked: bool,
    major: Major,
    utf8: Utf8,
}

fn text(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    begin(state, header, Major::Text)
}

fn bytes(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    begin(state, header, Major::Bytes)
}

fn begin(state: &mut State, header: Header, major: Major) -> Result<Flow, ErrorKind> {
    let DecKind::Owned { begin, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    if header.major != major {
        return Err(ErrorKind::TypeMismatch);
    }
    let chunked = header.indefinite();
    unsafe { begin(state.target.dst) };
    state.scratch.write(Scratch {
        copy: Copying {
            left: if chunked { 0 } else { chunk(header)? },
            total: 0,
            chunked,
            major,
            utf8: Utf8::new(),
        },
    });
    state.step = copy;
    Ok(Flow::Continue)
}

/// Copies the payload as it arrives, then each chunk of an indefinite string up to its break.
fn copy(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    let DecKind::Owned { append, finish, .. } = state.target.kind else {
        return Err(ErrorKind::Unsupported);
    };
    let dst = state.target.dst;
    loop {
        let copying = unsafe { &mut state.scratch.assume_init_mut().copy };
        let segment = input.take(copying.left);
        let taken = u64::try_from(segment.len()).unwrap_or(copying.left);
        if cfg!(cbor_static_oneshot) && taken < copying.left {
            return Err(ErrorKind::UnexpectedEnd);
        }
        unsafe { append(dst, copying.total, segment) }?;
        if copying.major == Major::Text {
            copying.utf8.feed(segment)?;
        }
        copying.total += segment.len();
        copying.left -= taken;
        if copying.left > 0 {
            return Ok(Flow::NeedInput);
        }
        copying.utf8.finish()?;
        if copying.chunked {
            let Some(header) = state.carry.header(input, state.strict)? else {
                return Ok(Flow::NeedInput);
            };
            let copying = unsafe { &mut state.scratch.assume_init_mut().copy };
            match (header.is_break(), header.major == copying.major) {
                (false, true) => {
                    copying.left = chunk(header)?;
                    continue;
                }
                (false, false) => return Err(ErrorKind::NotWellFormed),
                (true, _) => {}
            }
        }
        let total = unsafe { state.scratch.assume_init_ref().copy.total };
        unsafe { finish(dst, total) }?;
        return complete(state, frames).map(|()| Flow::Continue);
    }
}

/// A definite chunk's length, which must also fit `usize` (R-103).
fn chunk(header: Header) -> Result<u64, ErrorKind> {
    if header.indefinite() {
        return Err(ErrorKind::NotWellFormed);
    }
    length(header.arg).map(|_| header.arg)
}

/// The form never holds a frame, so it is never the parent of a completed entry.
fn never_a_parent(_: &mut Frame) -> Result<After, ErrorKind> {
    Err(ErrorKind::Unsupported)
}
