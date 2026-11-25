//! `cbor_static::serde` against the native layer, on types that derive both.

use cbor_static::{Decode, Encode, ErrorKind};
use serde::{Deserialize, Serialize};

use crate::Refused;

/// Checks `$input` as each type deriving both layers, by [`agree`].
#[macro_export]
macro_rules! layers {
    ($input:expr, [$($ty:ty),* $(,)?]) => {{
        let input: &[u8] = $input;
        $(
            $crate::serde_layer::agree(
                input,
                core::any::type_name::<$ty>(),
                |bytes| $crate::whole::<$ty>(bytes),
                |bytes| $crate::serde_layer::whole::<$ty>(bytes),
            );
        )*
    }};
}

/// Decodes `$input` through the serde layer as each type, which must not panic: for serde's
/// buffered forms, whose decoding follows serde's rules rather than the native layer's.
#[macro_export]
macro_rules! deserializes {
    ($input:expr, [$($ty:ty),* $(,)?]) => {{
        let input: &[u8] = $input;
        $(
            drop(cbor_static::serde::from_slice::<$ty>(input));
        )*
    }};
}

/// Both layers accept `input` as the type `name` or both refuse it for the same kind of error,
/// and what they accept both encoders write as the same bytes. serde's buffered forms are left
/// out, which the generated types do not use (their decoding follows serde's rules); so is the
/// kind of a refusal cbor-static's `serde_divergences` tests pin, by [`refused_apart`]: both
/// layers must still refuse it.
///
/// # Panics
///
/// When the layers disagree.
pub fn agree(
    input: &[u8],
    name: &str,
    native: impl Fn(&[u8]) -> Result<Vec<u8>, Refused>,
    serde: impl Fn(&[u8]) -> Result<[Vec<u8>; 2], Refused>,
) {
    match (native(input), serde(input)) {
        (Ok(native), Ok(serde)) => assert_eq!(
            [&native, &native],
            serde.each_ref(),
            "{name}: the native layer writes what it decodes from {input:02x?} as {native:02x?}, \
             and the native and serde encoders write what the serde layer decodes as {serde:02x?}"
        ),
        (Err(native), Err(serde)) if refused_apart(input, native, serde) => {}
        (Err(native), Err(serde)) => assert_eq!(
            native.kind, serde.kind,
            "{name}: the native layer refuses {input:02x?} as {native:?}, the serde layer as {serde:?}"
        ),
        (native, serde) => panic!(
            "{name}: the native layer decodes {input:02x?} as {native:02x?}, the serde layer as {serde:02x?}"
        ),
    }
}

/// Whether the layers refuse `input` apart as one of cbor-static's `serde_divergences` pins
/// does, where serde's Deserializer is not told what the native leaf knows.
#[must_use]
pub fn refused_apart(input: &[u8], native: Refused, serde: Refused) -> bool {
    assembled_borrowed_string(native, serde)
        || pending_items_overflow(input, native, serde)
        || fixed_array_count(input, native, serde)
}

/// `a_valid_indefinite_text_into_a_borrowed_str` and its rows: the native borrowed leaf refuses an
/// indefinite string at its head; with `alloc` the serde layer assembles it first, as
/// `deserialize_str` cannot tell whether the visitor borrows.
fn assembled_borrowed_string(native: Refused, serde: Refused) -> bool {
    native.kind == ErrorKind::IndefiniteString && serde.offset >= native.offset
}

/// `a_map_counted_past_2_63_into_a_struct` and `a_key_array_counted_2_64_minus_1_into_a_struct`,
/// and their rows: both layers keep pending items in a `u64` and overflow at different places
/// (the serde layer's tokens double a map's count; native adds the value to a skipped key's
/// count), so one refuses at an array or map head counting at least 2^63 and the other later.
fn pending_items_overflow(input: &[u8], native: Refused, serde: Refused) -> bool {
    [(native, serde), (serde, native)]
        .into_iter()
        .any(|(limited, other)| {
            limited.kind == ErrorKind::LimitExceeded
                && other.offset >= limited.offset
                && usize::try_from(limited.offset)
                    .ok()
                    .and_then(|end| input.get(end.checked_sub(9)?..end))
                    .and_then(|head| head.split_first())
                    .is_some_and(|(&initial, count)| {
                        matches!(initial, 0x9b | 0xbb)
                            && <[u8; 8]>::try_from(count)
                                .is_ok_and(|count| u64::from_be_bytes(count) >= 1 << 63)
                    })
        })
}

/// `a_long_array_into_a_fixed_array` and its rows: a native `[T; N]` checks an array's count at
/// its head, where the serde layer, which gives arrays and tuples the same `deserialize_tuple`,
/// checks it after the elements.
fn fixed_array_count(input: &[u8], native: Refused, serde: Refused) -> bool {
    native.kind == ErrorKind::TypeMismatch
        && matches!(
            serde.kind,
            ErrorKind::TypeMismatch | ErrorKind::MissingField | ErrorKind::UnexpectedEnd
        )
        && serde.offset >= native.offset
        && usize::try_from(native.offset).is_ok_and(|at| array_ends(input).contains(&at))
}

/// The offsets that end an array's head or an indefinite array's break, as far as `Tokens` reads
/// `input`.
fn array_ends(input: &[u8]) -> Vec<usize> {
    use cbor_static::{Token, Tokens};

    Tokens::new(input)
        .map_while(Result::ok)
        .scan((0, Vec::new()), |(at, indefinite), token| {
            *at += input.get(*at).map_or(0, |&initial| head_len(initial))
                + match token {
                    Token::Bytes(bytes) => bytes.len(),
                    Token::Text(text) => text.len(),
                    _ => 0,
                };
            let ends = match token {
                Token::Array(len) => {
                    if len.is_none() {
                        indefinite.push(true);
                    }
                    true
                }
                Token::Map(None) | Token::BytesIndefinite | Token::TextIndefinite => {
                    indefinite.push(false);
                    false
                }
                Token::Break => indefinite.pop() == Some(true),
                _ => false,
            };
            Some(ends.then_some(*at))
        })
        .flatten()
        .collect()
}

/// The length of the head whose first byte is `initial`.
fn head_len(initial: u8) -> usize {
    match initial & 0x1f {
        24 => 2,
        25 => 3,
        26 => 5,
        27 => 9,
        _ => 1,
    }
}

/// `input` decoded at once through the serde layer as `T`, written by the native encoder and by
/// the serde layer's.
///
/// # Errors
///
/// The decode error.
///
/// # Panics
///
/// When an encoder cannot write the decoded value.
pub fn whole<'de, T: Decode<'de> + Encode + Deserialize<'de> + Serialize>(
    input: &'de [u8],
) -> Result<[Vec<u8>; 2], Refused> {
    cbor_static::serde::from_slice::<T>(input)
        .map(|value| {
            [
                crate::written(&value),
                crate::grown(|out| cbor_static::serde::to_slice(&value, out))
                    .expect("the serde layer writes what it decodes"),
            ]
        })
        .map_err(Refused::from)
}

#[cfg(test)]
mod tests {
    use super::{array_ends, refused_apart};
    use crate::Refused;
    use cbor_static::ErrorKind;

    fn refused(kind: ErrorKind, offset: u32) -> Refused {
        Refused {
            kind,
            field: 0,
            offset,
        }
    }

    /// One test per input: [`array_ends`] finds exactly `$ends`.
    macro_rules! array_ends_at {
        ($($name:ident: $input:expr, $ends:expr;)*) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(array_ends($input), $ends);
                }
            )*
        };
    }

    array_ends_at! {
        a_definite_head_and_an_indefinite_head_and_break_end_arrays: b"\x82\x01\x9f\x02\xff", [1, 3, 5];
        a_two_byte_array_head_ends_past_its_argument: b"\x98\x01\x65hello", [2];
        an_indefinite_arrays_break_ends_it_inside_a_map: b"\xbf\x61a\x9f\xff\xff", [4, 5];
        an_indefinite_byte_strings_break_ends_no_array: b"\x5f\x41a\xff", [0_usize; 0];
    }

    /// One test per refusal pair: [`refused_apart`] says `$apart`.
    macro_rules! apart {
        ($($name:ident: $input:expr, $native:expr, $serde:expr, $apart:literal;)*) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(refused_apart($input, $native, $serde), $apart);
                }
            )*
        };
    }

    const HUGE_MAP: &[u8] = b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63";

    apart! {
        a_map_counted_past_2_63_running_out: HUGE_MAP, refused(ErrorKind::UnexpectedEnd, 9), refused(ErrorKind::LimitExceeded, 9), true;
        a_map_counted_past_2_63_with_an_invalid_key: b"\xbb\xfa\x67\x17\x18\x03\x2d\x72\x63\x61\xff", refused(ErrorKind::InvalidUtf8, 11), refused(ErrorKind::LimitExceeded, 9), true;
        a_map_counted_below_2_63_is_not_apart: b"\xbb\x7f\xff\xff\xff\xff\xff\xff\xff", refused(ErrorKind::UnexpectedEnd, 9), refused(ErrorKind::LimitExceeded, 9), false;
        native_refusing_before_a_huge_maps_end_is_not_apart: HUGE_MAP, refused(ErrorKind::TypeMismatch, 1), refused(ErrorKind::LimitExceeded, 9), false;
        a_key_array_counted_2_64_minus_1: b"\xa1\x9b\xff\xff\xff\xff\xff\xff\xff\xff\xff", refused(ErrorKind::LimitExceeded, 10), refused(ErrorKind::NotWellFormed, 11), true;
        serde_refusing_before_a_huge_key_arrays_end_is_not_apart: b"\xa1\x9b\xff\xff\xff\xff\xff\xff\xff\xff\xff", refused(ErrorKind::LimitExceeded, 10), refused(ErrorKind::TypeMismatch, 1), false;
        a_limit_not_at_a_huge_head_is_not_apart: b"\x82\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00", refused(ErrorKind::LimitExceeded, 10), refused(ErrorKind::NotWellFormed, 11), false;
        a_long_array_into_a_fixed_array: b"\x83\x01\x02\x03", refused(ErrorKind::TypeMismatch, 1), refused(ErrorKind::TypeMismatch, 3), true;
        native_refusing_inside_an_array_is_not_apart: b"\x83\x01\x02\x03", refused(ErrorKind::TypeMismatch, 2), refused(ErrorKind::TypeMismatch, 3), false;
        serde_refusing_before_the_array_head_is_not_apart: b"\x83\x01\x02\x03", refused(ErrorKind::TypeMismatch, 1), refused(ErrorKind::UnexpectedEnd, 0), false;
        serde_refusing_an_array_otherwise_is_not_apart: b"\x82\x62\xc3\x28", refused(ErrorKind::TypeMismatch, 1), refused(ErrorKind::InvalidUtf8, 4), false;
        an_indefinite_string_into_a_borrowed_one: b"\x7f\xff", refused(ErrorKind::IndefiniteString, 1), refused(ErrorKind::TypeMismatch, 2), true;
        serde_refusing_before_an_indefinite_strings_head_is_not_apart: b"\x7f\xff", refused(ErrorKind::IndefiniteString, 1), refused(ErrorKind::UnexpectedEnd, 0), false;
        a_string_cut_short_is_not_apart: b"\x41", refused(ErrorKind::TypeMismatch, 1), refused(ErrorKind::UnexpectedEnd, 1), false;
    }
}
