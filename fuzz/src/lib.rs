//! What every fuzz target asserts of the bytes libFuzzer hands it: each target names its types,
//! and these checks hold for any input.

pub mod oracle;
pub mod serde_layer;
pub mod tokens;

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "../../cbor-static-conformance/tests/common/differential/mod.rs"]
mod oracles;

use cbor_static::{Decode, Encode, Error, ErrorKind};

/// A decode error, comparable whichever way it was reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refused {
    pub kind: ErrorKind,
    pub field: u16,
    pub offset: u32,
}

impl From<Error> for Refused {
    fn from(error: Error) -> Self {
        Self {
            kind: error.kind,
            field: error.field,
            offset: error.offset,
        }
    }
}
/// Checks `$input` as each type, by [`typed`]; a type that borrows is written with `'_`.
#[macro_export]
macro_rules! typed {
    ($input:expr, [$($ty:ty),* $(,)?]) => {{
        let input: &[u8] = $input;
        $(
            $crate::typed(
                input,
                core::any::type_name::<$ty>(),
                <$ty as cbor_static::Decode<'_>>::CONTIGUOUS,
                |bytes| $crate::whole::<$ty>(bytes),
                |bytes, at| $crate::two_chunks::<$ty>(bytes, at),
            );
        )*
    }};
}

/// Checks `$input` as each EDHOC CBOR sequence type, by [`sequence`].
#[macro_export]
macro_rules! sequences {
    ($input:expr, [$($ty:ty),* $(,)?]) => {{
        let input: &[u8] = $input;
        $(
            $crate::sequence(input, core::any::type_name::<$ty>(), |bytes| {
                $crate::whole_sequence::<$ty>(bytes)
            });
        )*
    }};
}

/// Decoding `input` as the type `name` never panics; what decodes writes bytes that decode and
/// write the same bytes again; and fed in two chunks it decodes as at once, value or error alike.
/// A type that needs contiguous input may refuse the split as `NeedContiguous`.
///
/// # Panics
///
/// When one of those fails.
pub fn typed(
    input: &[u8],
    name: &str,
    contiguous: bool,
    whole: impl Fn(&[u8]) -> Result<Vec<u8>, Refused>,
    two_chunks: impl Fn(&[u8], usize) -> Result<Vec<u8>, Refused>,
) {
    let once = whole(input);
    if let Ok(written) = &once {
        assert_eq!(
            whole(written).as_ref(),
            Ok(written),
            "{name} rewrites its own bytes {written:02x?}, decoded from {input:02x?}"
        );
    }
    let at = split_point(input);
    match two_chunks(input, at) {
        Err(error) if contiguous && error.kind == ErrorKind::NeedContiguous => {}
        chunked => assert_eq!(
            chunked, once,
            "{name} decodes {input:02x?} split at {at} otherwise than at once"
        ),
    }
}

/// [`typed`]'s fix-point for an EDHOC CBOR sequence, which decodes only whole.
///
/// # Panics
///
/// When the bytes a decoded sequence writes do not decode and write the same bytes again.
pub fn sequence(input: &[u8], name: &str, whole: impl Fn(&[u8]) -> Result<Vec<u8>, Error>) {
    if let Ok(written) = whole(input) {
        assert_eq!(
            whole(&written),
            Ok(written.clone()),
            "{name} rewrites its own bytes {written:02x?}, decoded from {input:02x?}"
        );
    }
}

/// `input` decoded at once as `T`, written again.
///
/// # Errors
///
/// The decode error.
pub fn whole<'de, T: Decode<'de> + Encode>(input: &'de [u8]) -> Result<Vec<u8>, Refused> {
    cbor_static::from_slice::<T>(input)
        .map(|value| written(&value))
        .map_err(Refused::from)
}

/// `input` fed to a [`Decoder`] as `input[..at]` then `input[at..]`, written again; the errors
/// for trailing or missing input are `from_slice`'s.
///
/// # Errors
///
/// The decode error.
pub fn two_chunks<'de, T: Decode<'de> + Encode>(
    input: &'de [u8],
    at: usize,
) -> Result<Vec<u8>, Refused> {
    use cbor_static::{Decoder, Fed};
    use core::mem::MaybeUninit;

    let (head, tail) = input.split_at(at);
    let mut slot = MaybeUninit::uninit();
    let mut decoder = Decoder::<T>::new(&mut slot);
    let used = match decoder.feed(head)? {
        Fed::Done(used) => Some(used),
        Fed::NeedInput if tail.is_empty() => None,
        Fed::NeedInput => match decoder.feed(tail)? {
            Fed::Done(used) => Some(at + used),
            Fed::NeedInput => None,
        },
    };
    match used {
        Some(used) if used == input.len() => decoder
            .finish()
            .map(|value| written(&*value))
            .map_err(Refused::from),
        Some(used) => Err(Refused::at(ErrorKind::TrailingInput, used)),
        None => Err(Refused::at(ErrorKind::UnexpectedEnd, input.len())),
    }
}

/// `input` decoded as the EDHOC sequence `T`, written again.
///
/// # Errors
///
/// The decode error.
///
/// # Panics
///
/// When a decoded sequence cannot be written.
pub fn whole_sequence<'de, T: cbor_static_conformance::edhoc::Sequence<'de>>(
    input: &'de [u8],
) -> Result<Vec<u8>, Error> {
    T::from_sequence(input)
        .map(|value| grown(|out| value.to_sequence(out)).expect("a decoded sequence writes itself"))
}

/// The bytes `value` writes.
///
/// # Panics
///
/// When a decoded value cannot be written.
pub fn written<T: Encode>(value: &T) -> Vec<u8> {
    cbor_static::to_vec(value).expect("a decoded value writes itself")
}

/// What `write` writes, into a buffer grown until it fits.
///
/// # Errors
///
/// Any error but [`ErrorKind::BufferTooSmall`].
///
/// # Panics
///
/// When no buffer up to `usize::MAX` bytes fits.
pub fn grown(write: impl Fn(&mut [u8]) -> Result<usize, Error>) -> Result<Vec<u8>, Error> {
    core::iter::successors(Some(256_usize), |len| len.checked_mul(2))
        .map(|len| {
            let mut out = vec![0; len];
            write(&mut out).map(|used| {
                out.truncate(used);
                out
            })
        })
        .find(|written| !matches!(written, Err(error) if error.kind == ErrorKind::BufferTooSmall))
        .expect("some buffer fits")
}

/// Where to split `input` in two: anywhere from before its first byte to after its last, as its
/// bytes decide, so the fuzzer reaches every split.
#[must_use]
pub fn split_point(input: &[u8]) -> usize {
    input
        .iter()
        .fold(0_usize, |hash, &byte| {
            hash.wrapping_mul(31).wrapping_add(usize::from(byte))
        })
        .checked_rem(input.len() + 1)
        .unwrap_or_default()
}

impl Refused {
    fn at(kind: ErrorKind, offset: usize) -> Self {
        Self {
            kind,
            field: 0,
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Refused, split_point, two_chunks, whole};
    use cbor_static::ErrorKind;

    #[test]
    fn empty_input_splits_before_nothing() {
        assert_eq!(split_point(b""), 0);
    }

    #[test]
    fn the_split_point_is_within_the_input() {
        assert!((0..=64).all(|len| split_point(&vec![0xab; len]) <= len));
    }

    /// One test per decode: it gives `$expected`.
    macro_rules! decodes {
        ($($name:ident: $decoded:expr, $expected:expr;)*) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!($decoded, $expected);
                }
            )*
        };
    }

    decodes! {
        two_chunks_of_an_item_cut_short_end_unexpectedly: two_chunks::<[u8; 2]>(b"\x82\x01", 1), Err(Refused::at(ErrorKind::UnexpectedEnd, 2));
        two_chunks_past_the_item_trail: two_chunks::<u8>(b"\x01\x02", 1), Err(Refused::at(ErrorKind::TrailingInput, 1));
        an_item_split_before_its_head_decodes_whole: two_chunks::<[u8; 2]>(b"\x82\x01\x02", 0), whole::<[u8; 2]>(b"\x82\x01\x02");
        an_item_split_after_its_head_decodes_whole: two_chunks::<[u8; 2]>(b"\x82\x01\x02", 1), whole::<[u8; 2]>(b"\x82\x01\x02");
        an_item_split_between_elements_decodes_whole: two_chunks::<[u8; 2]>(b"\x82\x01\x02", 2), whole::<[u8; 2]>(b"\x82\x01\x02");
        an_item_split_after_its_end_decodes_whole: two_chunks::<[u8; 2]>(b"\x82\x01\x02", 3), whole::<[u8; 2]>(b"\x82\x01\x02");
        an_item_with_more_after_it_trails: whole::<u8>(b"\x01\x02"), Err(Refused::at(ErrorKind::TrailingInput, 1));
        an_item_cut_short_ends_unexpectedly: whole::<[u8; 2]>(b"\x82\x01"), Err(Refused::at(ErrorKind::UnexpectedEnd, 2));
    }
}
