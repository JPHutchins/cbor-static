//! R-103: wire lengths become `usize` through one checked conversion, which `--cfg
//! cbor_static_usize16` caps at `u16::MAX`. Streaming skips never convert, so they pass any length.

mod common;

use cbor_static::to_slice;
use common::{beyond_16_bits, chunked_decode, decode};

const LIMIT: usize = u16::MAX as usize;

#[derive(cbor_static::Cbor, Clone, Copy, Debug, PartialEq)]
struct Probe {
    #[serde(skip_serializing_if = "Option::is_none")]
    a: Option<u32>,
}

#[derive(cbor_static::Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "t")]
enum Kind {
    A,
}

#[test]
fn strings_at_the_16_bit_limit_decode() {
    assert_eq!(
        decode::<&[u8]>(&encoded(&vec![0; LIMIT].as_slice())),
        Ok(vec![0; LIMIT].as_slice())
    );
    assert_eq!(
        decode::<&str>(&encoded(&"a".repeat(LIMIT).as_str())),
        Ok("a".repeat(LIMIT).as_str())
    );
}

#[test]
fn strings_one_past_the_16_bit_limit_exceed_it_only_under_usize16() {
    assert_eq!(
        decode::<&[u8]>(&encoded(&vec![0; LIMIT + 1].as_slice())),
        beyond_16_bits(vec![0; LIMIT + 1].as_slice())
    );
    assert_eq!(
        decode::<&str>(&encoded(&"a".repeat(LIMIT + 1).as_str())),
        beyond_16_bits("a".repeat(LIMIT + 1).as_str())
    );
}

#[test]
fn streaming_skips_pass_any_length() {
    let input = [
        b"\xa1\x61x".as_slice(),
        &encoded(&vec![0; LIMIT + 1].as_slice()),
    ]
    .concat();
    assert_eq!(decode::<Probe>(&input), Ok(Probe { a: None }));
    for chunk in [7, 4096].into_iter().filter(|_| !cfg!(miri)) {
        assert_eq!(
            chunked_decode::<Probe>(&input, chunk),
            Ok(Probe { a: None }),
            "{chunk}"
        );
    }
}

#[test]
fn contiguous_scans_convert_every_length() {
    let tagged = |len: usize| {
        [
            b"\xa2\x61x".as_slice(),
            &encoded(&vec![0; len].as_slice()),
            b"\x61t\x61A",
        ]
        .concat()
    };
    assert_eq!(decode::<Kind>(&tagged(LIMIT)), Ok(Kind::A));
    assert_eq!(decode::<Kind>(&tagged(LIMIT + 1)), beyond_16_bits(Kind::A));
}

fn encoded<T: cbor_static::Encode>(value: &T) -> Vec<u8> {
    let mut out = vec![0; 2 * LIMIT + 16];
    let len = to_slice(value, &mut out).expect("encodes");
    out.truncate(len);
    out
}

/// #84: an `other` field that counts its map's unknown entries skips each as a struct without one
/// does, past any length, so it decodes alike in any chunks.
mod counted {
    use cbor_static::{Decode, ErrorKind, Skipped};

    #[derive(cbor_static::Decode, Clone, Copy, Debug, Default, PartialEq)]
    struct Text {
        id: Option<u8>,
        #[cbor(other)]
        rest: Skipped,
    }

    #[derive(cbor_static::Decode, Clone, Copy, Debug, Default, PartialEq)]
    struct Int {
        #[cbor(key = 1)]
        id: Option<u8>,
        #[cbor(other)]
        rest: Skipped,
    }

    #[derive(cbor_static::Decode, Clone, Copy, Debug, Default, PartialEq)]
    struct Mixed {
        #[cbor(key = 1)]
        id: Option<u8>,
        name: Option<u8>,
        #[cbor(other)]
        rest: Skipped,
    }

    macro_rules! cases {
        ($($name:ident: $type:ident, $bytes:expr => $expected:expr;)*) => {$(
            #[test]
            fn $name() {
                assert_eq!(in_any_chunks(&$bytes, |value: $type| value.rest.count()), $expected);
            }
        )*};
    }

    cases! {
        a_byte_string_key_of_a_text_keyed_map: Text, past_the_limit(b"\xa1", b"\x01", true) => Ok(1);
        a_byte_string_key_of_an_integer_keyed_map: Int, past_the_limit(b"\xa1", b"\x01", true) => Ok(1);
        a_byte_string_key_of_a_mixed_map: Mixed, past_the_limit(b"\xa1", b"\x01", true) => Ok(1);
        a_text_key_of_a_text_keyed_map: Text, a_text_key_past_the_limit(true) => Ok(1);
        a_text_key_of_a_mixed_map: Mixed, a_text_key_past_the_limit(true) => Ok(1);
        a_value_of_a_text_keyed_map: Text, past_the_limit(b"\xa1\x61z", b"", true) => Ok(1);
        a_value_of_an_integer_keyed_map: Int, past_the_limit(b"\xa1\x02", b"", true) => Ok(1);
        a_value_of_a_mixed_map: Mixed, past_the_limit(b"\xa1\x02", b"", true) => Ok(1);
        a_string_nested_in_a_value: Int, past_the_limit(b"\xa1\x02\x81", b"", true) => Ok(1);
        a_cut_byte_string_key: Int, past_the_limit(b"\xa1", b"\x01", false) => Err((ErrorKind::UnexpectedEnd, 0));
        a_cut_text_key: Text, a_text_key_past_the_limit(false) => Err((ErrorKind::UnexpectedEnd, 0));
        a_cut_value: Mixed, past_the_limit(b"\xa1\x02", b"", false) => Err((ErrorKind::UnexpectedEnd, 0));
        a_cut_nested_string: Int, past_the_limit(b"\xa1\x02\x81", b"", false) => Err((ErrorKind::UnexpectedEnd, 0));
    }

    /// A capture needs each entry whole in one piece, which a 16-bit target cannot address.
    #[cfg(any(feature = "heapless", feature = "alloc"))]
    macro_rules! captured {
        ($($name:ident: $capture:ty;)*) => {$(
            #[test]
            fn $name() {
                #[derive(cbor_static::Decode, Debug)]
                struct Captured<'a> {
                    #[cbor(key = 1)]
                    id: Option<u8>,
                    #[cbor(other)]
                    rest: $capture,
                }

                assert_eq!(
                    cbor_static::from_slice::<Captured>(&past_the_limit(b"\xa1\x02", b"", true))
                        .map(|captured| captured.rest.len())
                        .map_err(|e| e.kind),
                    super::beyond_16_bits(1)
                );
            }
        )*};
    }

    #[cfg(feature = "heapless")]
    captured! {
        a_heapless_capture_exceeds_the_limit_only_under_usize16: heapless::Vec<cbor_static::RawEntry<'a>, 4>;
    }

    #[cfg(feature = "alloc")]
    captured! {
        an_allocated_capture_exceeds_the_limit_only_under_usize16: Vec<cbor_static::RawEntry<'a>>;
    }

    /// The count `bytes` decodes to, or its error's kind and field, at once and in two pieces cut at
    /// each of its first bytes and inside its long payload, which all agree.
    fn in_any_chunks<T: for<'de> Decode<'de> + Default>(
        bytes: &[u8],
        count: fn(T) -> u32,
    ) -> Result<u32, (ErrorKind, u16)> {
        use core::mem::MaybeUninit;

        use cbor_static::{Decoder, Fed};

        let whole = cbor_static::from_slice::<T>(bytes)
            .map(count)
            .map_err(|e| (e.kind, e.field));
        for at in (0..=bytes.len().min(12))
            .chain([bytes.len() - 1])
            .filter(|at| !cfg!(miri) || *at == 2)
        {
            let mut slot = MaybeUninit::uninit();
            let mut decoder = Decoder::<T>::new(&mut slot);
            assert_eq!(
                <[&[u8]; 2]>::from(bytes.split_at(at))
                    .into_iter()
                    .try_for_each(|piece| decoder.feed(piece).map(|_: Fed| ()))
                    .and_then(|()| decoder.finish().map(|value| count(core::mem::take(value))))
                    .map_err(|e| (e.kind, e.field)),
                whole,
                "split at {at}"
            );
        }
        whole
    }

    /// One unknown entry, `before` then a byte string one past the 16-bit limit, then `after`: the
    /// string whole, or cut short two bytes into its payload.
    fn past_the_limit(before: &[u8], after: &[u8], complete: bool) -> Vec<u8> {
        [
            before,
            b"\x5a\x00\x01\x00\x00",
            &vec![0; if complete { super::LIMIT + 1 } else { 2 }],
            if complete { after } else { &[] },
        ]
        .concat()
    }

    /// A text key one past the 16-bit limit, then its value.
    fn a_text_key_past_the_limit(complete: bool) -> Vec<u8> {
        [
            b"\xa1\x7a\x00\x01\x00\x00".as_slice(),
            &vec![b'a'; if complete { super::LIMIT + 1 } else { 2 }],
            if complete { b"\x01" } else { &[] },
        ]
        .concat()
    }
}
