//! An encoding stopped at an error by a form whose open can fail on the value itself (#34): the
//! encoder restarts at a root whose open fails with that error again, so each later resume returns
//! it and writes nothing. Only such forms link this; `resume` itself is unchanged.

use core::mem::MaybeUninit;
use core::ptr;

use super::option::frameless;
use super::{Child, EncOps, State};
use crate::engine::pending::Pending;
use crate::error::ErrorKind;
use crate::schema::EncKind;
use crate::stack::Frame;

static STOPPED: EncOps = EncOps {
    open,
    next: frameless,
};

static ROOT: EncKind = EncKind::Select { ops: &STOPPED };

/// Stops the encoding at `kind`, keeping the depth and offset its error reports.
pub(crate) fn stop(state: &mut State, kind: &'static ErrorKind) -> ErrorKind {
    state.root = &ROOT;
    state.value = ptr::from_ref(kind).cast();
    state.started = false;
    state.pending = Pending::new(None);
    *kind
}

/// Fails with the kind the root holds, stopping again, since beginning the root marked the
/// encoding started.
fn open(
    state: &mut State,
    _: &mut [MaybeUninit<Frame>],
    _: &'static EncKind,
    at: *const u8,
) -> Result<Option<Child>, ErrorKind> {
    Err(stop(state, unsafe { &*at.cast::<ErrorKind>() }))
}

/// `kind` where [`stop`] can keep it.
pub(crate) const fn promoted(kind: ErrorKind) -> &'static ErrorKind {
    match kind {
        ErrorKind::UnexpectedEnd => &ErrorKind::UnexpectedEnd,
        ErrorKind::NotWellFormed => &ErrorKind::NotWellFormed,
        ErrorKind::TypeMismatch => &ErrorKind::TypeMismatch,
        ErrorKind::OutOfRange => &ErrorKind::OutOfRange,
        ErrorKind::InvalidUtf8 => &ErrorKind::InvalidUtf8,
        ErrorKind::DuplicateKey => &ErrorKind::DuplicateKey,
        ErrorKind::MissingField => &ErrorKind::MissingField,
        ErrorKind::UnknownVariant => &ErrorKind::UnknownVariant,
        ErrorKind::UnexpectedTag => &ErrorKind::UnexpectedTag,
        ErrorKind::IndefiniteString => &ErrorKind::IndefiniteString,
        ErrorKind::NeedContiguous => &ErrorKind::NeedContiguous,
        ErrorKind::Unsupported => &ErrorKind::Unsupported,
        ErrorKind::LimitExceeded => &ErrorKind::LimitExceeded,
        ErrorKind::BufferTooSmall => &ErrorKind::BufferTooSmall,
        ErrorKind::TrailingInput => &ErrorKind::TrailingInput,
        ErrorKind::UnknownField => &ErrorKind::UnknownField,
        ErrorKind::NotDeterministic => &ErrorKind::NotDeterministic,
        ErrorKind::OutOfMemory => &ErrorKind::OutOfMemory,
        #[cfg(feature = "serde")]
        ErrorKind::Custom => &ErrorKind::Custom,
    }
}
