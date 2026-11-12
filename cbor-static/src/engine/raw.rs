//! Items borrowed whole from the input and checked once when decoded (D-20, D-11a): any item as
//! [`RawCbor`], and arrays and maps as [`ArrayRef`] and [`MapRef`], whose every element was decoded
//! as its type, so iterating them cannot fail. They need contiguous input, and encode as the bytes
//! they hold, verbatim.
#![expect(
    clippy::cast_ptr_alignment,
    reason = "every field pointer is a descriptor offset taken with offset_of!, so it is aligned"
)]

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use super::input::Input;
use super::{DECODE_RAW as RAW, Decoder, Fed, Item, Pending, StrictOps};
use crate::error::ErrorKind;
use crate::lead::Lead;
use crate::schema::{DecKind, Decode, EncKind, Encode};
use crate::stack::Level;
use crate::wire::{INDEFINITE, Major};

/// One well-formed item, borrowed from the input.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Envelope<'a> {
///     kind: u32,
///     body: cbor_static::RawCbor<'a>,
/// }
///
/// let input = b"\xa2\x64kind\x01\x64body\x82\x18\x18\x01";
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let envelope: &mut Envelope = cbor_static::decode_into(input, &mut slot)?;
/// assert_eq!(envelope.body.as_bytes(), b"\x82\x18\x18\x01");
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::encode_into(&*envelope, &mut out)?;
/// assert_eq!(&out[..len], input);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawCbor<'de> {
    bytes: &'de [u8],
}

impl<'de> RawCbor<'de> {
    /// The item's encoding, head included.
    #[must_use]
    pub const fn as_bytes(&self) -> &'de [u8] {
        self.bytes
    }
}

/// An array of `T`, every element checked to decode when the array was decoded.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let values: &mut cbor_static::ArrayRef<u16> = cbor_static::decode_into(b"\x9f\x01\x19\x01\x00\xff", &mut slot)?;
/// assert_eq!(values.len(), 2);
/// assert!(values.iter().eq([1, 256]));
///
/// let mut slot = core::mem::MaybeUninit::<cbor_static::ArrayRef<u16>>::uninit();
/// assert_eq!(
///     cbor_static::decode_into(b"\x82\x01\x61a", &mut slot).map(|_| ()).map_err(|e| e.kind),
///     Err(cbor_static::ErrorKind::TypeMismatch)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[repr(C)]
pub struct ArrayRef<'de, T> {
    raw: RawCbor<'de>,
    len: usize,
    elem: PhantomData<fn() -> T>,
}

/// A map of `K` to `V`, every entry checked to decode when the map was decoded.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let map: &mut cbor_static::MapRef<&str, u8> = cbor_static::decode_into(b"\xa2\x61a\x01\x61b\x02", &mut slot)?;
/// assert!(map.iter().eq([("a", 1), ("b", 2)]));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[repr(C)]
pub struct MapRef<'de, K, V> {
    raw: RawCbor<'de>,
    len: usize,
    entry: PhantomData<fn() -> (K, V)>,
}

impl<T> ArrayRef<'_, T> {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'de, T> ArrayRef<'de, T> {
    /// The array's encoding, head included.
    #[must_use]
    pub const fn as_bytes(&self) -> &'de [u8] {
        self.raw.bytes
    }
}

impl<'de, T: Decode<'de>> ArrayRef<'de, T> {
    #[must_use]
    pub fn iter(&self) -> ArrayIter<'de, T> {
        ArrayIter {
            rest: after_head(self.raw.bytes),
            left: self.len,
            elem: PhantomData,
        }
    }
}

impl<'de, T: Decode<'de>> IntoIterator for &ArrayRef<'de, T> {
    type Item = T;
    type IntoIter = ArrayIter<'de, T>;

    fn into_iter(self) -> ArrayIter<'de, T> {
        self.iter()
    }
}

impl<K, V> MapRef<'_, K, V> {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'de, K, V> MapRef<'de, K, V> {
    /// The map's encoding, head included.
    #[must_use]
    pub const fn as_bytes(&self) -> &'de [u8] {
        self.raw.bytes
    }
}

impl<'de, K: Decode<'de>, V: Decode<'de>> MapRef<'de, K, V> {
    #[must_use]
    pub fn iter(&self) -> MapIter<'de, K, V> {
        MapIter {
            rest: after_head(self.raw.bytes),
            left: self.len,
            entry: PhantomData,
        }
    }
}

impl<'de, K: Decode<'de>, V: Decode<'de>> IntoIterator for &MapRef<'de, K, V> {
    type Item = (K, V);
    type IntoIter = MapIter<'de, K, V>;

    fn into_iter(self) -> MapIter<'de, K, V> {
        self.iter()
    }
}

/// The elements of an [`ArrayRef`], decoded one by one.
pub struct ArrayIter<'de, T> {
    rest: &'de [u8],
    left: usize,
    elem: PhantomData<fn() -> T>,
}

/// The entries of a [`MapRef`], decoded one by one.
pub struct MapIter<'de, K, V> {
    rest: &'de [u8],
    left: usize,
    entry: PhantomData<fn() -> (K, V)>,
}

impl<'de, T: Decode<'de>> Iterator for ArrayIter<'de, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.left = self.left.checked_sub(1)?;
        let (value, rest) = first::<T>(self.rest)?;
        self.rest = rest;
        Some(value)
    }
}

impl<'de, K: Decode<'de>, V: Decode<'de>> Iterator for MapIter<'de, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<(K, V)> {
        self.left = self.left.checked_sub(1)?;
        let (key, rest) = first::<K>(self.rest)?;
        let (value, rest) = first::<V>(rest)?;
        self.rest = rest;
        Some((key, value))
    }
}

/// The item at the front of `bytes`, which decoding has already checked, and the bytes after it.
fn first<'de, T: Decode<'de>>(bytes: &'de [u8]) -> Option<(T, &'de [u8])> {
    let mut slot = MaybeUninit::<T>::uninit();
    let used = {
        let mut decoder = Decoder::new(&mut slot);
        let Ok(Fed::Done(used)) = decoder.feed(bytes) else {
            return None;
        };
        decoder.finish().ok()?;
        used
    };
    Some((unsafe { slot.assume_init() }, bytes.get(used..)?))
}

/// An array's or map's elements, after its head.
fn after_head(bytes: &[u8]) -> &[u8] {
    let mut input = Input::new(bytes);
    match input.take_head() {
        Ok(Some(_)) => input.rest(),
        _ => &[],
    }
}

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
    let mut slot = MaybeUninit::<T>::uninit();
    let mut decoder = Decoder::under(&mut slot, strict);
    match decoder.feed(bytes).map_err(|error| error.kind)? {
        Fed::Done(used) => Ok(used),
        Fed::NeedInput => Err(ErrorKind::UnexpectedEnd),
    }
}

/// `bytes` for the input's lifetime, which the decoder's own `'de` guarantees.
unsafe fn borrowed<'de>(bytes: &[u8]) -> &'de [u8] {
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
