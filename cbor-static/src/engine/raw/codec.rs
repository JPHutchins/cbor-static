//! How the views are checked and written when decoded, and written back verbatim when encoded.
#![expect(
    clippy::cast_ptr_alignment,
    reason = "every field pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use core::marker::PhantomData;

use super::{ArrayRef, MapRef, RawCbor, after_head};
use crate::engine::{DECODE_RAW as RAW, Item, Pending, StrictOps};
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::schema::{DecKind, Decode, EncKind, Encode};
use crate::stack::Level;
use crate::wire::{INDEFINITE, Major};

#[expect(clippy::unnecessary_wraps, reason = "a StoreFn can fail")]
unsafe fn store_raw(dst: *mut u8, item: &Item<'_>) -> Result<(), ErrorKind> {
    unsafe {
        dst.cast::<RawCbor>().write(RawCbor {
            bytes: borrowed(item.payload),
        });
    }
    Ok(())
}

unsafe fn store_array<'de, T: Decode<'de>>(dst: *mut u8, item: &Item<'_>) -> Result<(), ErrorKind> {
    let bytes = unsafe { borrowed(item.payload) };
    let len = entries(item, Major::Array, |rest| {
        used::<T>(unsafe { borrowed(rest) }, item.strict)
    })?;
    unsafe {
        dst.cast::<ArrayRef<'de, T>>().write(ArrayRef {
            raw: RawCbor { bytes },
            len,
            elem: PhantomData,
        });
    }
    Ok(())
}

unsafe fn store_map<'de, K: Decode<'de>, V: Decode<'de>>(
    dst: *mut u8,
    item: &Item<'_>,
) -> Result<(), ErrorKind> {
    let bytes = unsafe { borrowed(item.payload) };
    let len = entries(item, Major::Map, |rest| {
        let key = used::<K>(unsafe { borrowed(rest) }, item.strict)?;
        Ok(key
            + used::<V>(
                unsafe { borrowed(rest.get(key..).unwrap_or(&[])) },
                item.strict,
            )?)
    })?;
    unsafe {
        dst.cast::<MapRef<'de, K, V>>().write(MapRef {
            raw: RawCbor { bytes },
            len,
            entry: PhantomData,
        });
    }
    Ok(())
}

/// Counts the entries of the container `item` of `major`, each of which `entry` checks at the front
/// of the bytes left, returning how many it took.
fn entries(
    item: &Item<'_>,
    major: Major,
    mut entry: impl FnMut(&[u8]) -> Result<usize, ErrorKind>,
) -> Result<usize, ErrorKind> {
    if item.header.major != major {
        return Err(ErrorKind::TypeMismatch);
    }
    let mut rest = after_head(item.payload);
    let mut count = 0;
    while !ended(item, rest, count) {
        rest = rest.get(entry(rest)?..).unwrap_or(&[]);
        count += 1;
    }
    Ok(count)
}

/// Whether a container has no entries left: its break, or its count reached.
fn ended(item: &Item<'_>, rest: &[u8], count: usize) -> bool {
    if item.header.indefinite() {
        rest.first() == Some(&BREAK)
    } else {
        u64::try_from(count) == Ok(item.header.arg)
    }
}

/// How many bytes the `T` at the front of `bytes` takes, decoded under the profile `strict` and then
/// dropped. A nested decoder lives on the machine stack for it, which the type bounds: how deeply
/// raw views nest in `T` fixes how deeply this recurses (#12 Q4).
fn used<'de, T: Decode<'de>>(
    bytes: &'de [u8],
    strict: Option<&'static StrictOps>,
) -> Result<usize, ErrorKind> {
    use core::mem::MaybeUninit;

    use crate::engine::{Decoder, Fed};

    let mut slot = MaybeUninit::<T>::uninit();
    let mut decoder = Decoder::under(&mut slot, strict);
    match decoder.feed(bytes).map_err(|error| error.kind)? {
        Fed::Done(used) => Ok(used),
        Fed::NeedInput => Err(ErrorKind::UnexpectedEnd),
    }
}

/// `bytes` for the input's lifetime, which the decoder's own `'de` guarantees.
pub(super) unsafe fn borrowed<'de>(bytes: &[u8]) -> &'de [u8] {
    unsafe { core::slice::from_raw_parts(bytes.as_ptr(), bytes.len()) }
}

const BREAK: u8 = (Major::Simple as u8) << 5 | INDEFINITE;

unsafe fn enc_raw(src: *const u8, out: &mut Pending) {
    out.tail(unsafe { src.cast::<RawCbor>().read() }.bytes);
}

unsafe impl Encode for RawCbor<'_> {
    const ENCODE: &'static EncKind = &ENC_RAW;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

unsafe impl<'de: 'a, 'a> Decode<'de> for RawCbor<'a> {
    const DECODE: &'static DecKind = &DecKind::Raw {
        ops: &RAW,
        store: store_raw,
        drop: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::ANY;
}

unsafe impl<T> Encode for ArrayRef<'_, T> {
    const ENCODE: &'static EncKind = &ENC_RAW;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

unsafe impl<'de: 'a, 'a, T: Decode<'a>> Decode<'de> for ArrayRef<'a, T> {
    const DECODE: &'static DecKind = &DecKind::Raw {
        ops: &RAW,
        store: store_array::<T>,
        drop: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::ARRAY;
}

unsafe impl<K, V> Encode for MapRef<'_, K, V> {
    const ENCODE: &'static EncKind = &ENC_RAW;
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

unsafe impl<'de: 'a, 'a, K: Decode<'a>, V: Decode<'a>> Decode<'de> for MapRef<'a, K, V> {
    const DECODE: &'static DecKind = &DecKind::Raw {
        ops: &RAW,
        store: store_map::<K, V>,
        drop: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = Level<()>;
    const LEAD: Lead = Lead::MAP;
}

static ENC_RAW: EncKind = EncKind::Leaf {
    encode: enc_raw,
    absent: None,
};
