#![forbid(unsafe_code)]
//! Engine stack storage derived from the type (D-14): one [`Frame`] per nesting level.

use core::mem::{ManuallyDrop, MaybeUninit, align_of, size_of};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Key,
    /// A map's key, while its internally tagged enum's tag entry is still ahead.
    TagAhead,
    Value,
    EnumUnit,
    EnumName,
    EnumPayload,
    EnumDone,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Frame {
    pub(crate) remaining: u64,
    pub(crate) seen: u64,
    pub(crate) desc: *const (),
    pub(crate) base: *mut u8,
    pub(crate) field: u8,
    pub(crate) phase: Phase,
}

pub(crate) const INDEFINITE: u64 = u64::MAX;

#[repr(C)]
pub struct Level<Children> {
    _frame: MaybeUninit<Frame>,
    _children: MaybeUninit<Children>,
}

#[repr(C)]
pub union Widest<A, B> {
    _a: ManuallyDrop<A>,
    _b: ManuallyDrop<B>,
}

/// Two stacks side by side: room to encode two values at once.
#[cfg(feature = "heapless")]
#[repr(C)]
pub struct Twice<S> {
    _first: MaybeUninit<S>,
    _second: MaybeUninit<S>,
}

/// A stack's levels below its first: those of a map-form struct's fields, which a struct that
/// flattens it holds in its own frame's place (#14 Q34).
pub trait Unlevel {
    type Children;
}

impl<Children> Unlevel for Level<Children> {
    type Children = Children;
}

#[must_use]
pub const fn deeper(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

/// The frames `S` holds.
pub(crate) const fn depth_of<S>() -> usize {
    size_of::<S>() / size_of::<Frame>()
}

pub(crate) const fn fits<S>(depth: usize) -> bool {
    size_of::<S>() == depth * size_of::<Frame>()
        && (depth == 0 || align_of::<S>() == align_of::<Frame>())
}

#[cfg(test)]
mod tests {
    use super::{Frame, Level, Widest, deeper, fits};

    type Leaf = Level<Widest<(), ()>>;
    type Mid = Level<Widest<Leaf, ()>>;
    type Top = Level<Widest<Mid, Widest<Leaf, ()>>>;

    #[test]
    fn stack_size_is_depth_times_frame() {
        assert!(fits::<()>(0));
        assert!(fits::<Leaf>(1));
        assert!(fits::<Mid>(2));
        assert!(fits::<Top>(1 + deeper(2, 1)));
        assert!(!fits::<Top>(2));
        assert_eq!(size_of::<Top>(), 3 * size_of::<Frame>());
    }
}
