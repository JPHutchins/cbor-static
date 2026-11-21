//! An `other` field's captured entries (R-128), in its struct's map: each key, then its value, as
//! they arrived (D-11a), with no head of their own, since the map's head counts them. The frame's
//! `base` is the first entry, `remaining` the count, `seen` the entries written, `field` the
//! `other` field's, and its phase whether an entry's key or value is next.

use core::mem::{MaybeUninit, offset_of};

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::{RawEntry, push};
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};

pub static ENTRIES: EncOps = EncOps { open, next };

/// The entries before one whose key is claimed or repeated, whose turn stops the encoding there.
static REFUSING: EncOps = EncOps {
    open,
    next: refusing,
};

static REFUSED: EncKind = EncKind::Select { ops: &REFUSING };

/// Opens the captured entries: all of them, or those before the first whose key a field of their
/// struct claims or an earlier entry holds.
#[expect(
    clippy::cast_ptr_alignment,
    reason = "a capture's view is its vector's buffer of entries"
)]
fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    use core::slice;

    use crate::engine::frame_mut;

    let EncKind::Seq { view, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    let (first, len) = unsafe { view(at) };
    let parent = frame_mut(
        frames,
        state.depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?,
    )?;
    let other = parent.field;
    let (kind, count) = match refused(
        unsafe { slice::from_raw_parts(first.cast::<RawEntry<'_>>(), len) },
        desc(parent),
    )? {
        Some(index) => (&REFUSED, index),
        None => (kind, len),
    };
    push(
        frames,
        &mut state.depth,
        Frame {
            remaining: u64::try_from(count).unwrap_or(u64::MAX),
            ..opened(kind, first, other, Phase::Key)
        },
    )
    .map(|()| None)
}

/// The first of `entries` whose key a field of `parent`, their struct, claims, or an earlier entry
/// holds.
#[inline(never)]
fn refused(entries: &[RawEntry<'_>], parent: &EncKind) -> Result<Option<usize>, ErrorKind> {
    use crate::engine::{distinct, distinct_from};

    let EncKind::Struct { layout, .. } = parent else {
        return Err(ErrorKind::Unsupported);
    };
    entries
        .iter()
        .enumerate()
        .find_map(|(index, entry)| {
            (0..layout.fields.len())
                .filter_map(|field| u8::try_from(field).ok())
                .try_for_each(|field| distinct_from(entry.key, layout.name_item(field)))
                .and_then(|()| distinct(entries.get(..index).unwrap_or_default(), entry.key))
                .err()
                .map(|kind| (index, kind))
        })
        .map_or(Ok(None), |(index, kind)| match kind {
            ErrorKind::DuplicateKey => Ok(Some(index)),
            kind => Err(kind),
        })
}

fn next(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    Ok(entry_part(frame)?.map_or(Part::Finished, Part::Child))
}

fn refusing(_: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    entry_part(frame)?
        .map(Part::Child)
        .ok_or(ErrorKind::DuplicateKey)
}

fn entry_part(frame: &mut Frame) -> Result<Option<Child>, ErrorKind> {
    use crate::engine::RawCbor;
    use crate::schema::Encode;

    if frame.seen >= frame.remaining {
        return Ok(None);
    }
    let entry = frame.base.cast_const().wrapping_add(
        usize::try_from(frame.seen).map_err(|_| ErrorKind::Unsupported)?
            * size_of::<RawEntry<'static>>(),
    );
    Ok(Some(Child {
        kind: <RawCbor<'static> as Encode>::ENCODE,
        at: if frame.phase == Phase::Key {
            frame.phase = Phase::Value;
            entry.wrapping_add(offset_of!(RawEntry<'static>, key))
        } else {
            frame.phase = Phase::Key;
            frame.seen += 1;
            entry.wrapping_add(offset_of!(RawEntry<'static>, value))
        },
    }))
}
