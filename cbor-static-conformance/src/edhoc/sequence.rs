use cbor_static::{Decode, Encode, Error, ErrorKind};
use heapless::Vec;

/// The most EAD items one message carries.
pub(crate) const EAD_ITEMS: usize = 4;

/// A group of items written one after another, as in an RFC 8742 CBOR sequence, with no array
/// around them. The derive writes a struct as one item, so each group composes its items with
/// [`cbor_static::from_slice_partial`]. An error's offset is into the whole sequence.
///
/// ```
/// use cbor_static::ErrorKind;
/// use cbor_static_conformance::edhoc::{Info, Message1, Sequence};
///
/// let written = b"\x0a\x40\x18\x20";
/// let info = Info::from_sequence(written)?;
/// assert_eq!((info.label, info.context, info.length), (10, b"".as_slice(), 32));
/// let mut out = [0u8; 8];
/// let len = info.to_sequence(&mut out)?;
/// assert_eq!(&out[..len], written);
/// assert_eq!(
///     Info::from_sequence(b"\x0a\x40\x18\x20\x00").map_err(|e| (e.kind, e.offset)),
///     Err((ErrorKind::TrailingInput, 4))
/// );
/// assert_eq!(
///     Message1::from_sequence(b"\x03\x02\x61a\x0e").map_err(|e| (e.kind, e.offset)),
///     Err((ErrorKind::TypeMismatch, 4))
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub trait Sequence<'a>: Sized {
    /// The group at the front of `input`, and the bytes after it.
    ///
    /// # Errors
    ///
    /// When an item is malformed or of the wrong type, or input ends inside the group.
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error>;

    /// Writes the group's items into `out` from `at`, and returns where they end.
    ///
    /// ```
    /// use cbor_static_conformance::edhoc::{Ead, Sequence};
    ///
    /// let mut out = [0xffu8; 4];
    /// let end = Ead { label: 0, value: Some(b"\xe9") }.encode_at(&mut out, 1)?;
    /// assert_eq!((end, out), (4, [0xff, 0x00, 0x41, 0xe9]));
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// When `out` is too small.
    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error>;

    /// The group that is the whole of `input`.
    ///
    /// # Errors
    ///
    /// As [`Sequence::decode_front`], and when items follow the group.
    fn from_sequence(input: &'a [u8]) -> Result<Self, Error> {
        Self::decode_front(input).and_then(|(group, rest)| {
            if rest.is_empty() {
                Ok(group)
            } else {
                Err(placed(Error::from(ErrorKind::TrailingInput), input, rest))
            }
        })
    }

    /// Writes the group into `out`, and returns its length.
    ///
    /// # Errors
    ///
    /// When `out` is too small.
    fn to_sequence(&self, out: &mut [u8]) -> Result<usize, Error> {
        self.encode_at(out, 0)
    }
}

/// RFC 9528 §3.8's `ead`: a label, and a value when a byte string follows it.
///
/// ```
/// use cbor_static_conformance::edhoc::{Ead, Sequence};
///
/// let (padding, rest) = Ead::decode_front(b"\x00\x41\xe9\x20")?;
/// assert_eq!((padding, rest), (Ead { label: 0, value: Some(b"\xe9") }, b"\x20".as_slice()));
/// assert_eq!(Ead::decode_front(b"\x20")?.0, Ead { label: -1, value: None });
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ead<'a> {
    pub label: i64,
    pub value: Option<&'a [u8]>,
}

impl<'a> Sequence<'a> for Ead<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        item::<i64>(input, input).and_then(|(label, rest)| {
            if starts_with(rest, 2) {
                item::<&[u8]>(input, rest).map(|(value, rest)| {
                    (
                        Self {
                            label,
                            value: Some(value),
                        },
                        rest,
                    )
                })
            } else {
                Ok((Self { label, value: None }, rest))
            }
        })
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        put(&self.label, out, at)
            .and_then(|at| self.value.map_or(Ok(at), |value| put(&value, out, at)))
    }
}

/// RFC 9528's `EAD_x = (+ ead)` where a message may end in one: the EAD items while integers
/// follow, none when nothing does. Its minimum of one is not checked, since an absent `EAD_x`
/// and an empty one are the same bytes.
///
/// ```
/// use cbor_static_conformance::edhoc::{Eads, Sequence};
///
/// assert!(Eads::from_sequence(b"")?.is_empty());
/// let (eads, rest) = Eads::decode_front(b"\x00\x40\x01\xa0")?;
/// assert_eq!((eads.len(), rest), (2, b"\xa0".as_slice()));
/// assert_eq!(
///     Eads::from_sequence(b"\x00\x01\x02\x03\x04").map_err(|e| (e.kind, e.offset)),
///     Err((cbor_static::ErrorKind::LimitExceeded, 4))
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type Eads<'a> = Vec<Ead<'a>, EAD_ITEMS>;

impl<'a> Sequence<'a> for Eads<'a> {
    fn decode_front(input: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        eads_after(Self::new(), input, input)
    }

    fn encode_at(&self, out: &mut [u8], at: usize) -> Result<usize, Error> {
        self.iter().try_fold(at, |at, ead| ead.encode_at(out, at))
    }
}

/// `read`, then the EAD items at the front of `rest`.
fn eads_after<'a>(
    mut read: Eads<'a>,
    whole: &'a [u8],
    rest: &'a [u8],
) -> Result<(Eads<'a>, &'a [u8]), Error> {
    if starts_with(rest, 0) || starts_with(rest, 1) {
        group::<Ead>(whole, rest).and_then(move |(ead, after)| {
            read.push(ead)
                .map_err(|_| placed(Error::from(ErrorKind::LimitExceeded), whole, rest))?;
            eads_after(read, whole, after)
        })
    } else {
        Ok((read, rest))
    }
}

/// One item at `rest`, which `whole` ends with.
pub(crate) fn item<'a, T: Decode<'a>>(
    whole: &'a [u8],
    rest: &'a [u8],
) -> Result<(T, &'a [u8]), Error> {
    cbor_static::from_slice_partial(rest).map_err(|error| placed(error, whole, rest))
}

/// One group at `rest`, which `whole` ends with.
pub(crate) fn group<'a, T: Sequence<'a>>(
    whole: &'a [u8],
    rest: &'a [u8],
) -> Result<(T, &'a [u8]), Error> {
    T::decode_front(rest).map_err(|error| placed(error, whole, rest))
}

/// Writes one item into `out` from `at`, and returns where it ends.
pub(crate) fn put<T: Encode>(value: &T, out: &mut [u8], at: usize) -> Result<usize, Error> {
    out.get_mut(at..)
        .ok_or_else(|| Error::from(ErrorKind::BufferTooSmall))
        .and_then(|rest| cbor_static::to_slice(value, rest))
        .map(|len| at + len)
}

/// The last item of `input`, as written; all of it when it holds one or none.
pub(crate) fn last_item(input: &[u8]) -> &[u8] {
    core::iter::successors(Some(input), |rest| {
        cbor_static::from_slice_partial::<cbor_static::RawCbor>(rest)
            .ok()
            .map(|(_, after)| after)
            .filter(|after| !after.is_empty())
    })
    .last()
    .unwrap_or(input)
}

/// The error of an item at `rest`, placed in `whole`.
pub(crate) fn placed(mut error: Error, whole: &[u8], rest: &[u8]) -> Error {
    error.offset = error
        .offset
        .saturating_add(u32::try_from(whole.len() - rest.len()).unwrap_or(u32::MAX));
    error
}

fn starts_with(input: &[u8], major: u8) -> bool {
    input.first().is_some_and(|head| head >> 5 == major)
}
