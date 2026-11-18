//! An ordered map (alloc `BTreeMap`): a map of its entries in its keys' order, each found from the
//! key before it, or under a deterministic root (D-11b), in the order of their keys' encodings, as a
//! `LinearMap`'s. The frame's `seen` counts whole entries, its `remaining` holds the address of the
//! last key written, and its phase says whether the next entry's key or value is next.

use core::mem::MaybeUninit;

use super::{Child, EncOps, Part, State, desc, linear_map, opened};
use crate::engine::pending::Pending;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::{Frame, Phase};

pub static BTREE_MAP: EncOps = EncOps { open, next };

fn open(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    kind: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    use crate::engine::push;
    use crate::wire::Major;

    let EncKind::Ordered { len, .. } = kind else {
        return Err(ErrorKind::Unsupported);
    };
    state.pending.head(
        Major::Map,
        u64::try_from(unsafe { len(at) }).unwrap_or(u64::MAX),
    );
    push(frames, &mut state.depth, opened(kind, at, 0, Phase::Key)).map(|()| None)
}

fn next(pending: &mut Pending, frame: &mut Frame) -> Result<Part, ErrorKind> {
    if pending.deterministic().is_some() {
        return linear_map::next(pending, frame);
    }
    let EncKind::Ordered {
        key, value, after, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    let (at_key, at_value) = unsafe { after(frame.base, last(frame.remaining)?) };
    if at_key.is_null() {
        return Ok(Part::Finished);
    }
    if frame.phase == Phase::Value {
        frame.phase = Phase::Key;
        frame.seen += 1;
        frame.remaining =
            u64::try_from(at_key.expose_provenance()).map_err(|_| ErrorKind::Unsupported)?;
        return Ok(Part::Child(Child {
            kind: value,
            at: at_value,
        }));
    }
    frame.field = u8::try_from(frame.seen).unwrap_or(u8::MAX);
    frame.phase = Phase::Value;
    Ok(Part::Child(Child {
        kind: key,
        at: at_key,
    }))
}

/// The last key written, from its address, or null before the first.
fn last(address: u64) -> Result<*const u8, ErrorKind> {
    use core::ptr;

    usize::try_from(address)
        .map(ptr::with_exposed_provenance)
        .map_err(|_| ErrorKind::Unsupported)
}
