//! A map of up to a capacity of entries (heapless `LinearMap`): a map of its entries in their order,
//! or under a deterministic root (D-11b), in the order of their keys' encodings. The frame's `seen`
//! counts whole entries, its `remaining` is the index of the entry being written, and its phase
//! says whether that entry's key or value is next.

use core::cmp::Ordering;
use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, opened};
use crate::engine::pending::Pending;
use crate::engine::push;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};
use crate::wire::Major;

pub static LINEAR_MAP: EncOps = EncOps { open, next };

/// The select kind, whose one user is a deterministic map (Q33): its frame encodes this in place of
/// each key, and the open names the key to encode instead, the smallest whose encoding sorts after
/// the last one written. Only `remaining` remembers that last entry, so the sort takes no scratch,
/// at O(N²) comparisons for a bounded N.
pub(super) static SELECT: EncKind = EncKind::Select {
    ops: &EncOps {
        open: select,
        next: super::option::frameless,
    },
};

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let EncKind::Map { len, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    state.pending.head(
        Major::Map,
        u64::try_from(unsafe { len(at) }).unwrap_or(u64::MAX),
    );
    push(frames, &mut state.depth, opened(kind, at, 0, Phase::Key)).map(|()| None)
}

fn next(pending: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    let EncKind::Map {
        key, value, entry, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.phase == Phase::Value {
        frame.phase = Phase::Key;
        frame.seen += 1;
        return Ok(Part::Child(Child {
            kind: value,
            at: unsafe { entry(frame.base, index(frame.remaining)?) }.1,
        }));
    }
    let (at_key, _) = unsafe { entry(frame.base, index(frame.seen)?) };
    if at_key.is_null() {
        return Ok(Part::Finished);
    }
    if let Some(deterministic) = pending.deterministic() {
        return Ok(Part::Child(Child {
            kind: deterministic.select,
            at: frame.base,
        }));
    }
    frame.remaining = frame.seen;
    frame.field = u8::try_from(frame.seen).unwrap_or(u8::MAX);
    frame.phase = Phase::Value;
    Ok(Part::Child(Child {
        kind: key,
        at: at_key,
    }))
}

/// Picks the next key of the map in the top frame, comparing encodings on the frames free above it.
fn select(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    _: &'static EncKind,
    _: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    let ops = state
        .pending
        .deterministic()
        .ok_or(ErrorKind::Unsupported)?;
    let (map, free) = frames
        .get_mut(usize::from(state.depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?)..)
        .and_then(<[_]>::split_first_mut)
        .ok_or(ErrorKind::Unsupported)?;
    let map = unsafe { map.assume_init_mut() };
    let EncKind::Map {
        key, len, entry, ..
    } = desc(map)
    else {
        return Err(ErrorKind::Unsupported);
    };
    let base = map.base.cast_const();
    let key_at = |at: usize| unsafe { entry(base, at) }.0;
    let last = match map.seen {
        0 => None,
        _ => Some(key_at(index(map.remaining)?)),
    };
    let chosen = (0..unsafe { len(base) })
        .try_fold(None, |best: Option<usize>, candidate| {
            let after_last = match last {
                None => true,
                Some(last) => {
                    (ops.order)(key, key_at(candidate), last, free, ops)? == Ordering::Greater
                }
            };
            let before_best = match best {
                Some(best) if after_last => {
                    (ops.order)(key, key_at(candidate), key_at(best), free, ops)? == Ordering::Less
                }
                _ => after_last,
            };
            Ok::<_, ErrorKind>(if before_best { Some(candidate) } else { best })
        })?
        .ok_or(ErrorKind::DuplicateKey)?;
    map.remaining = u64::try_from(chosen).map_err(|_| ErrorKind::Unsupported)?;
    map.field = u8::try_from(chosen).unwrap_or(u8::MAX);
    map.phase = Phase::Value;
    Ok(Some(Child {
        kind: key,
        at: key_at(chosen),
    }))
}

fn index(at: u64) -> Result<usize, ErrorKind> {
    usize::try_from(at).map_err(|_| ErrorKind::Unsupported)
}
