//! What a map-form struct's `#[cbor(other)]` field holds (R-128, #14 Q35): the entries of its map
//! that no field claims, counted, or captured whole and borrowed, which encode back verbatim
//! (D-11a). The kinds here are reached only from the `*_OTHER` map ops (D-05b).
#![expect(
    clippy::cast_ptr_alignment,
    reason = "a field pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use super::RawCbor;
use crate::engine::{ENCODE_SKIPPED, Item, Next, write_default};
use crate::error::ErrorKind;
use crate::schema::{DecKind, EncKind, Order, Other, sealed};

/// An entry of a map that no field claims, captured by `#[cbor(other)]`: its key and its value,
/// each the item it arrived as, which it encodes back as, verbatim (D-11a).
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let key: &mut cbor_static::RawCbor = cbor_static::decode_into(b"\x61z", &mut slot)?;
/// let entry = cbor_static::RawEntry { key: *key, value: *key };
/// assert_eq!(entry.key.as_bytes(), b"\x61z");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawEntry<'de> {
    pub key: RawCbor<'de>,
    pub value: RawCbor<'de>,
}

/// How many entries of a map no field claimed, counted by `#[cbor(other)]` up to `u32::MAX`,
/// which it then keeps. It is never written.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug)]
/// struct Header {
///     id: u8,
///     #[cbor(other)]
///     unknown: cbor_static::Skipped,
/// }
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let header: &mut Header = cbor_static::decode_into(b"\xa3\x01\x02\x62id\x07\x61z\xf5", &mut slot)?;
/// assert_eq!((header.id, header.unknown.count()), (7, 2));
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::encode_into(&*header, &mut out)?;
/// assert_eq!(&out[..len], b"\xa1\x62id\x07");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Skipped(u32);

impl Skipped {
    /// The entries counted.
    ///
    /// ```
    /// assert_eq!(cbor_static::Skipped::default().count(), 0);
    /// ```
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0
    }
}

impl sealed::Other for Skipped {}

impl Other for Skipped {
    const ENCODE_OTHER: &'static EncKind = &ENCODE_SKIPPED;
    const DECODE_OTHER: &'static DecKind = &DecKind::Leaf {
        decode: count,
        default: Some(write_default::<Self>),
        drop: None,
    };
    const ORDER: Order = Order::Any;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

/// Counts one more entry, which its map then skips.
#[expect(clippy::unnecessary_wraps, reason = "a DecodeFn")]
unsafe fn count(dst: *mut u8, _: &Item<'_>) -> Result<Next, ErrorKind> {
    unsafe {
        dst.cast::<Skipped>()
            .write(Skipped(dst.cast::<Skipped>().read().0.saturating_add(1)));
    }
    Ok(Next::Done)
}

#[cfg(feature = "heapless")]
mod captured {
    use heapless::{LenType, Vec};

    use super::super::RawCbor;
    use super::super::codec::borrowed;
    use super::RawEntry;
    use crate::engine::{ENCODE_ENTRIES, Item, Next, drop_fn, view, write_default};
    use crate::error::ErrorKind;
    use crate::schema::{DecKind, EncKind, Encode, Order, Other, sealed};
    use crate::stack::Level;

    impl<const N: usize, L: LenType> sealed::Other for Vec<RawEntry<'_>, N, L> {}

    /// Up to `N` entries no field claims, each taken whole from the chunk that holds it: an entry
    /// split across chunks is [`ErrorKind::NeedContiguous`], and one more than `N` is
    /// [`ErrorKind::LimitExceeded`]. They encode back in their field's place, verbatim; so under
    /// deterministic encoding, which they cannot be sorted for, they are a compile error.
    ///
    /// ```
    /// use cbor_static::RawEntry;
    ///
    /// #[derive(cbor_static::Cbor, Debug)]
    /// struct Header<'a> {
    ///     id: u8,
    ///     #[cbor(other)]
    ///     rest: heapless::Vec<RawEntry<'a>, 2>,
    /// }
    ///
    /// let input = b"\xa3\x62id\x07\x01\x02\x61z\xf5";
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let header: &mut Header = cbor_static::decode_into(input, &mut slot)?;
    /// assert_eq!(header.rest.len(), 2);
    /// assert_eq!(header.rest[1].key.as_bytes(), b"\x61z");
    ///
    /// let mut out = [0u8; 16];
    /// let len = cbor_static::encode_into(&*header, &mut out)?;
    /// assert_eq!(&out[..len], input);
    ///
    /// let mut slot = core::mem::MaybeUninit::<Header>::uninit();
    /// assert_eq!(
    ///     cbor_static::decode_into(b"\xa3\x01\x02\x03\x04\x05\x06", &mut slot).map(|_| ()).map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::LimitExceeded)
    /// );
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    impl<'de, const N: usize, L: LenType> Other for Vec<RawEntry<'de>, N, L> {
        const ENCODE_OTHER: &'static EncKind = &EncKind::Seq {
            ops: &ENCODE_ENTRIES,
            elem: <RawCbor<'de> as Encode>::ENCODE,
            stride: size_of::<RawEntry<'de>>(),
            view: view::<RawEntry<'de>, N, L>,
        };
        const DECODE_OTHER: &'static DecKind = &DecKind::Leaf {
            decode: capture::<N, L>,
            default: Some(write_default::<Self>),
            drop: drop_fn::<Self>(),
        };
        const ORDER: Order = Order::Conflict;
        type Stack = Level<()>;
        const MAX_DEPTH: usize = 1;
    }

    /// Captures the entry the payload holds whole, key then value; an empty payload is an entry this
    /// chunk does not hold whole.
    unsafe fn capture<const N: usize, L: LenType>(
        dst: *mut u8,
        item: &Item<'_>,
    ) -> Result<Next, ErrorKind> {
        use crate::engine::skip::item_len;

        let entry = unsafe { borrowed(item.payload) };
        if entry.is_empty() {
            return Err(ErrorKind::NeedContiguous);
        }
        let (key, value) = entry
            .split_at_checked(item_len(entry)?)
            .ok_or(ErrorKind::Unsupported)?;
        unsafe { &mut *dst.cast::<Vec<RawEntry<'_>, N, L>>() }
            .push(RawEntry {
                key: RawCbor { bytes: key },
                value: RawCbor { bytes: value },
            })
            .map_err(|_| ErrorKind::LimitExceeded)?;
        Ok(Next::Took(entry.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::{Skipped, count};
    use crate::engine::Item;
    use crate::wire::{Header, Major};

    #[test]
    fn the_count_saturates_at_its_most() {
        let mut skipped = Skipped(u32::MAX - 1);
        let item = Item {
            header: Header {
                major: Major::Uint,
                info: 0,
                arg: 0,
            },
            payload: &[],
            strict: None,
        };
        for _ in 0..2 {
            assert!(unsafe { count(core::ptr::from_mut(&mut skipped).cast(), &item) }.is_ok());
        }
        assert_eq!(skipped.count(), u32::MAX);
    }
}
