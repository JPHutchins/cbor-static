//! Deterministic encoding (RFC 8949 §4.2.1, or §4.2.3's length-first order), which a root type with
//! `#[cbor(deterministic)]` or `#[cbor(length_first)]` selects (D-11b). Derived keys are sorted at
//! compile time; these ops cover what only the encoder sees, so firmware without a deterministic
//! type links none of them (D-22).

#[cfg(feature = "heapless")]
use core::cmp::Ordering;
#[cfg(feature = "heapless")]
use core::mem::MaybeUninit;

use crate::engine::pending::Pending;
#[cfg(feature = "heapless")]
use crate::error::ErrorKind;
#[cfg(feature = "heapless")]
use crate::schema::EncKind;
#[cfg(feature = "heapless")]
use crate::stack::Frame;
use crate::wire::{Head, float};

pub struct DeterministicOps {
    /// Writes a float's shortest head, with every NaN as [`float::CANONICAL_NAN`].
    pub(crate) float: fn(&mut Pending, (u8, u64)),
    /// What a `LinearMap`'s frame encodes in place of its next key.
    #[cfg(feature = "heapless")]
    pub(crate) select: &'static EncKind,
    /// The order of two values' encodings.
    #[cfg(feature = "heapless")]
    pub(crate) order: Order,
}

/// The order of the encodings of two values of one kind, each encoded on half the free frames,
/// under the root's deterministic encoding.
#[cfg(feature = "heapless")]
pub(crate) type Order = fn(
    &'static EncKind,
    *const u8,
    *const u8,
    &mut [MaybeUninit<Frame>],
    &'static DeterministicOps,
) -> Result<Ordering, ErrorKind>;

pub static BYTEWISE: DeterministicOps = DeterministicOps {
    float: canonical,
    #[cfg(feature = "heapless")]
    select: &super::linear_map::SELECT,
    #[cfg(feature = "heapless")]
    order: |kind, a, b, frames, ops| super::bytewise(kind, a, b, frames, Some(ops)),
};
pub static LENGTH_FIRST: DeterministicOps = DeterministicOps {
    float: canonical,
    #[cfg(feature = "heapless")]
    select: &super::linear_map::SELECT,
    #[cfg(feature = "heapless")]
    order: length_first,
};

fn canonical(out: &mut Pending, shortest: (u8, u64)) {
    out.bytes(
        Head::float(if float::nan_head(shortest) {
            (float::HALF, float::CANONICAL_NAN)
        } else {
            shortest
        })
        .as_bytes(),
    );
}

/// RFC 8949 §4.2.3: the shorter encoding first, then bytewise.
#[cfg(feature = "heapless")]
fn length_first(
    kind: &'static EncKind,
    a: *const u8,
    b: *const u8,
    frames: &mut [MaybeUninit<Frame>],
    ops: &'static DeterministicOps,
) -> Result<Ordering, ErrorKind> {
    match super::encoded_len(kind, a, frames, Some(ops))?.cmp(&super::encoded_len(
        kind,
        b,
        frames,
        Some(ops),
    )?) {
        Ordering::Equal => super::bytewise(kind, a, b, frames, Some(ops)),
        unequal => Ok(unequal),
    }
}
