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
use crate::schema::{DecKind, EncKind, Order, Other, Payload, sealed};
#[cfg(any(feature = "heapless", feature = "alloc"))]
use crate::wire::Major;

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
/// which it then keeps. It is never written. It keeps no keys, so a repeated key counts again.
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
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let header: &mut Header = cbor_static::decode_into(b"\xa3\x20\x01\x62id\x07\x20\x02", &mut slot)?;
/// assert_eq!(header.unknown.count(), 2);
///
/// let mut out = [0u8; 8];
/// let len = cbor_static::to_slice(&*header, &mut out)?;
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
        payload: Payload::Refused,
    };
    const ORDER: Order = Order::Any;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    const CONTIGUOUS: bool = Self::DECODE_OTHER.takes_payload();
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

/// The entry the payload holds whole, key then value, and its length; an empty payload is an entry
/// this chunk does not hold whole.
///
/// # Safety
/// As [`borrowed`]: the payload outlives the entry.
#[cfg(any(feature = "heapless", feature = "alloc"))]
unsafe fn whole<'de>(item: &Item<'_>) -> Result<(RawEntry<'de>, usize), ErrorKind> {
    use super::codec::borrowed;
    use crate::engine::skip::item_len;

    let entry = unsafe { borrowed(item.payload) };
    if entry.is_empty() {
        return Err(ErrorKind::NeedContiguous);
    }
    let (key, value) = entry
        .split_at_checked(item_len(entry)?)
        .ok_or(ErrorKind::Unsupported)?;
    Ok((
        RawEntry {
            key: RawCbor { bytes: key },
            value: RawCbor { bytes: value },
        },
        entry.len(),
    ))
}

#[cfg(any(feature = "heapless", feature = "alloc"))]
#[inline(never)]
pub(crate) fn distinct(captured: &[RawEntry<'_>], key: RawCbor<'_>) -> Result<(), ErrorKind> {
    let wanted = identity(key)?;
    captured.iter().try_for_each(|earlier| {
        if identity(earlier.key)? == wanted {
            Err(ErrorKind::DuplicateKey)
        } else {
            Ok(())
        }
    })
}

/// Refuses `key` when it is the key `claimed` encodes, compared as a field's key is.
#[cfg(any(feature = "heapless", feature = "alloc"))]
#[inline(never)]
pub(crate) fn distinct_from(key: RawCbor<'_>, claimed: &[u8]) -> Result<(), ErrorKind> {
    if identity(key)? == identity(RawCbor { bytes: claimed })? {
        Err(ErrorKind::DuplicateKey)
    } else {
        Ok(())
    }
}

/// What two keys share when they are one key, as a field's key is matched.
#[cfg(any(feature = "heapless", feature = "alloc"))]
#[derive(PartialEq, Eq)]
struct KeyIdentity<'a> {
    major: Major,
    form: HeadForm,
    arg: u64,
    after_head: &'a [u8],
}

/// What a head's additional information says beyond its argument's width.
#[cfg(any(feature = "heapless", feature = "alloc"))]
#[derive(PartialEq, Eq)]
enum HeadForm {
    Definite,
    Indefinite,
    SimpleOrFloat(u8),
}

#[cfg(any(feature = "heapless", feature = "alloc"))]
#[inline(never)]
fn identity(key: RawCbor<'_>) -> Result<KeyIdentity<'_>, ErrorKind> {
    use crate::wire::{header_len, parse};

    let (head, after_head) = key
        .bytes
        .split_at_checked(header_len(
            *key.bytes.first().ok_or(ErrorKind::Unsupported)?,
        )?)
        .ok_or(ErrorKind::Unsupported)?;
    let header = parse(head)?;
    Ok(KeyIdentity {
        major: header.major,
        form: match (header.major, header.indefinite()) {
            (Major::Simple, _) => HeadForm::SimpleOrFloat(header.info),
            (_, true) => HeadForm::Indefinite,
            (_, false) => HeadForm::Definite,
        },
        arg: header.arg,
        after_head,
    })
}

#[cfg(feature = "heapless")]
mod captured {
    use heapless::{LenType, Vec};

    use super::super::RawCbor;
    use super::{RawEntry, distinct, whole};
    use crate::engine::{ENCODE_ENTRIES, Item, Next, drop_fn, view, write_default};
    use crate::error::ErrorKind;
    use crate::schema::{DecKind, EncKind, Encode, Order, Other, Payload, sealed};
    use crate::stack::Level;

    impl<const N: usize, L: LenType> sealed::Other for Vec<RawEntry<'_>, N, L> {}

    /// Up to `N` entries no field claims, each taken whole from the chunk that holds it: an entry
    /// split across chunks is [`ErrorKind::NeedContiguous`], one more than `N` is
    /// [`ErrorKind::LimitExceeded`], and a key it already holds is [`ErrorKind::DuplicateKey`].
    /// They encode back in their field's place, verbatim; so under deterministic encoding, which
    /// they cannot be sorted for, they are a compile error.
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
    /// let len = cbor_static::to_slice(&*header, &mut out)?;
    /// assert_eq!(&out[..len], input);
    ///
    /// let mut slot = core::mem::MaybeUninit::<Header>::uninit();
    /// assert_eq!(
    ///     cbor_static::decode_into(b"\xa3\x01\x02\x03\x04\x05\x06", &mut slot).map(|_| ()).map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::LimitExceeded)
    /// );
    ///
    /// let mut slot = core::mem::MaybeUninit::<Header>::uninit();
    /// assert_eq!(
    ///     cbor_static::decode_into(b"\xa3\x62id\x07\x20\x01\x20\x02", &mut slot).map(|_| ()).map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::DuplicateKey)
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
            payload: Payload::Taken,
        };
        const ORDER: Order = Order::Conflict;
        type Stack = Level<()>;
        const MAX_DEPTH: usize = 1;
        const CONTIGUOUS: bool = Self::DECODE_OTHER.takes_payload();
    }

    unsafe fn capture<const N: usize, L: LenType>(
        dst: *mut u8,
        item: &Item<'_>,
    ) -> Result<Next, ErrorKind> {
        let (entry, len) = unsafe { whole(item) }?;
        let captured = unsafe { &mut *dst.cast::<Vec<RawEntry<'_>, N, L>>() };
        distinct(captured, entry.key)?;
        captured.push(entry).map_err(|_| ErrorKind::LimitExceeded)?;
        Ok(Next::Took(len))
    }
}

#[cfg(feature = "alloc")]
mod allocated {
    use alloc::vec::Vec;

    use super::super::RawCbor;
    use super::{RawEntry, distinct, whole};
    use crate::engine::{ENCODE_ENTRIES, Item, Next, drop_fn, push_heap, view_heap, write_default};
    use crate::error::ErrorKind;
    use crate::schema::{DecKind, EncKind, Encode, Order, Other, Payload, sealed};
    use crate::stack::Level;

    impl sealed::Other for Vec<RawEntry<'_>> {}

    /// Every entry no field claims, each taken whole from the chunk that holds it: an entry split
    /// across chunks is [`ErrorKind::NeedContiguous`], and a key it already holds is
    /// [`ErrorKind::DuplicateKey`]. They encode back in their field's place, verbatim; so under
    /// deterministic encoding, which they cannot be sorted for, they are a compile error.
    ///
    /// ```
    /// use cbor_static::RawEntry;
    ///
    /// #[derive(cbor_static::Cbor, Debug)]
    /// struct Header<'a> {
    ///     id: u8,
    ///     #[cbor(other)]
    ///     rest: Vec<RawEntry<'a>>,
    /// }
    ///
    /// let input = b"\xa4\x62id\x07\x01\x02\x61z\xf5\x03\x80";
    /// let header: Header = cbor_static::from_slice(input)?;
    /// assert_eq!(header.rest.len(), 3);
    /// assert_eq!(header.rest[1].key.as_bytes(), b"\x61z");
    ///
    /// let mut out = [0u8; 16];
    /// let len = cbor_static::to_slice(&header, &mut out)?;
    /// assert_eq!(&out[..len], input);
    ///
    /// assert_eq!(
    ///     cbor_static::from_slice::<Header>(b"\xa3\x62id\x07\x20\x01\x20\x02").map(|_| ()).map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::DuplicateKey)
    /// );
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    impl<'de> Other for Vec<RawEntry<'de>> {
        const ENCODE_OTHER: &'static EncKind = &EncKind::Seq {
            ops: &ENCODE_ENTRIES,
            elem: <RawCbor<'de> as Encode>::ENCODE,
            stride: size_of::<RawEntry<'de>>(),
            view: view_heap::<RawEntry<'de>>,
        };
        const DECODE_OTHER: &'static DecKind = &DecKind::Leaf {
            decode: capture,
            default: Some(write_default::<Self>),
            drop: drop_fn::<Self>(),
            payload: Payload::Taken,
        };
        const ORDER: Order = Order::Conflict;
        type Stack = Level<()>;
        const MAX_DEPTH: usize = 1;
        const CONTIGUOUS: bool = Self::DECODE_OTHER.takes_payload();
    }

    unsafe fn capture(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
        let (entry, len) = unsafe { whole(item) }?;
        let captured = unsafe { &mut *dst.cast::<Vec<RawEntry<'_>>>() };
        distinct(captured, entry.key)?;
        push_heap(captured, entry)?;
        Ok(Next::Took(len))
    }
}

#[cfg(test)]
mod tests {
    use super::{Skipped, count};
    use crate::engine::Item;
    use crate::wire::{Header, Major};

    #[cfg(any(feature = "heapless", feature = "alloc"))]
    mod keys {
        use super::super::{RawCbor, RawEntry, distinct, distinct_from, identity};
        use crate::error::ErrorKind;

        fn same(a: &[u8], b: &[u8]) -> bool {
            identity(RawCbor { bytes: a }).expect("a key")
                == identity(RawCbor { bytes: b }).expect("a key")
        }

        macro_rules! cases {
            ($($name:ident: $a:expr, $b:expr => $same:expr;)*) => {$(
                #[test]
                fn $name() {
                    assert_eq!(same($a, $b), $same);
                }
            )*};
        }

        cases! {
            one_label_in_two_widths: b"\x20", b"\x38\x00" => true;
            one_label_in_the_widest_head: b"\x01", b"\x1b\x00\x00\x00\x00\x00\x00\x00\x01" => true;
            one_text_of_two_length_widths: b"\x61z", b"\x78\x01z" => true;
            one_tag_of_two_widths: b"\xc1\x00", b"\xd8\x01\x00" => true;
            an_unsigned_and_a_negative_label: b"\x00", b"\x20" => false;
            text_and_bytes: b"\x61z", b"\x41z" => false;
            two_texts: b"\x61z", b"\x61y" => false;
            false_and_a_half_float_of_its_bits: b"\xf4", b"\xf9\x00\x14" => false;
            an_empty_text_and_an_empty_indefinite_one: b"\x60", b"\x7f\xff" => false;
            one_float_in_two_widths: b"\xf9\x3c\x00", b"\xfa\x3f\x80\x00\x00" => false;
            a_nested_head_in_two_widths: b"\x81\x01", b"\x81\x18\x01" => false;
        }

        #[test]
        fn a_key_is_refused_only_when_held() {
            let held = [RawEntry {
                key: RawCbor { bytes: b"\x20" },
                value: RawCbor { bytes: b"\x01" },
            }];
            assert_eq!(
                (
                    distinct(&held, RawCbor { bytes: b"\x38\x00" }),
                    distinct(&held, RawCbor { bytes: b"\x21" }),
                    distinct(&[], RawCbor { bytes: b"\x20" }),
                ),
                (Err(ErrorKind::DuplicateKey), Ok(()), Ok(()))
            );
        }

        #[test]
        fn a_key_is_refused_only_when_claimed() {
            let key = RawCbor { bytes: b"\x01" };
            assert_eq!(
                (
                    distinct_from(key, b"\x18\x01"),
                    distinct_from(key, b"\x02"),
                    distinct_from(key, b""),
                ),
                (
                    Err(ErrorKind::DuplicateKey),
                    Ok(()),
                    Err(ErrorKind::Unsupported)
                )
            );
        }
    }

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
