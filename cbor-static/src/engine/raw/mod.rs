//! Items borrowed whole from the input and checked once when decoded (D-20, D-11a): any item as
//! [`RawCbor`], and arrays and maps as [`ArrayRef`] and [`MapRef`], whose every element was decoded
//! as its type, so iterating them cannot fail. They need contiguous input, and encode as the bytes
//! they hold, verbatim.

mod codec;

use core::marker::PhantomData;

use crate::schema::Decode;

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
    ///
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let raw: &mut cbor_static::RawCbor = cbor_static::decode_into(b"\x19\x00\x01", &mut slot)?;
    /// assert_eq!(raw.as_bytes(), b"\x19\x00\x01");
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
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
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let values: &mut cbor_static::ArrayRef<u8> = cbor_static::decode_into(b"\x9f\x01\x02\xff", &mut slot)?;
    /// assert_eq!(values.len(), 2);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let values: &mut cbor_static::ArrayRef<u8> = cbor_static::decode_into(b"\x80", &mut slot)?;
    /// assert!(values.is_empty());
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'de, T> ArrayRef<'de, T> {
    /// The array's encoding, head included.
    ///
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let values: &mut cbor_static::ArrayRef<u8> = cbor_static::decode_into(b"\x81\x18\x01", &mut slot)?;
    /// assert_eq!(values.as_bytes(), b"\x81\x18\x01");
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn as_bytes(&self) -> &'de [u8] {
        self.raw.bytes
    }
}

impl<'de, T: Decode<'de>> ArrayRef<'de, T> {
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let names: &mut cbor_static::ArrayRef<&str> = cbor_static::decode_into(b"\x82\x61a\x61b", &mut slot)?;
    /// assert!(names.iter().eq(["a", "b"]));
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
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
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let map: &mut cbor_static::MapRef<u8, u8> = cbor_static::decode_into(b"\xa1\x01\x02", &mut slot)?;
    /// assert_eq!(map.len(), 1);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let map: &mut cbor_static::MapRef<u8, u8> = cbor_static::decode_into(b"\xbf\xff", &mut slot)?;
    /// assert!(map.is_empty());
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'de, K, V> MapRef<'de, K, V> {
    /// The map's encoding, head included.
    ///
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let map: &mut cbor_static::MapRef<u8, u8> = cbor_static::decode_into(b"\xa1\x01\x02", &mut slot)?;
    /// assert_eq!(map.as_bytes(), b"\xa1\x01\x02");
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    #[must_use]
    pub const fn as_bytes(&self) -> &'de [u8] {
        self.raw.bytes
    }
}

impl<'de, K: Decode<'de>, V: Decode<'de>> MapRef<'de, K, V> {
    /// ```
    /// let mut slot = core::mem::MaybeUninit::uninit();
    /// let map: &mut cbor_static::MapRef<u8, bool> = cbor_static::decode_into(b"\xa1\x01\xf5", &mut slot)?;
    /// assert!(map.iter().eq([(1, true)]));
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
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
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let values: &mut cbor_static::ArrayRef<u8> = cbor_static::decode_into(b"\x82\x01\x02", &mut slot)?;
/// let mut elements: cbor_static::ArrayIter<u8> = values.iter();
/// assert_eq!([elements.next(), elements.next(), elements.next()], [Some(1), Some(2), None]);
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub struct ArrayIter<'de, T> {
    rest: &'de [u8],
    left: usize,
    elem: PhantomData<fn() -> T>,
}

/// The entries of a [`MapRef`], decoded one by one.
///
/// ```
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let map: &mut cbor_static::MapRef<u8, u8> = cbor_static::decode_into(b"\xa1\x01\x02", &mut slot)?;
/// let mut entries: cbor_static::MapIter<u8, u8> = map.iter();
/// assert_eq!([entries.next(), entries.next()], [Some((1, 2)), None]);
/// # Ok::<(), cbor_static::Error>(())
/// ```
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
    use core::mem::MaybeUninit;

    use super::{Decoder, Fed};

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
    use super::input::Input;

    let mut input = Input::new(bytes);
    match input.take_head() {
        Ok(Some(_)) => input.rest(),
        _ => &[],
    }
}
