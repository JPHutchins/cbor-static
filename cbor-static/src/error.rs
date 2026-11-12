#![forbid(unsafe_code)]

/// Why encoding or decoding stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[repr(u8)]
pub enum ErrorKind {
    UnexpectedEnd,
    NotWellFormed,
    TypeMismatch,
    OutOfRange,
    InvalidUtf8,
    DuplicateKey,
    MissingField,
    UnknownVariant,
    UnexpectedTag,
    IndefiniteString,
    NeedContiguous,
    Unsupported,
    LimitExceeded,
    BufferTooSmall,
    TrailingInput,
    UnknownField,
    /// A head, float, length, bignum or key order that RFC 8949 §4.2.1's deterministic encoding
    /// would not write, which the `Strict` profile rejects (D-12).
    ///
    /// ```
    /// let mut slot = core::mem::MaybeUninit::<u32>::uninit();
    /// assert_eq!(
    ///     cbor_static::decode_strict_into(b"\x18\x01", &mut slot).map_err(|e| e.kind),
    ///     Err(cbor_static::ErrorKind::NotDeterministic)
    /// );
    /// assert_eq!(*cbor_static::decode_into(b"\x18\x01", &mut slot)?, 1);
    /// # Ok::<(), cbor_static::Error>(())
    /// ```
    NotDeterministic,
}

/// An [`ErrorKind`], the index of the field being processed, and the byte offset reached.
///
/// ```
/// assert_eq!(size_of::<cbor_static::Error>(), 8);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Error {
    pub kind: ErrorKind,
    pub field: u16,
    pub offset: u32,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, field: u8, offset: usize) -> Self {
        Self {
            kind,
            field: u16::from(field),
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
        }
    }
}
