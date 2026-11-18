//! #5 Q18: `Embedded<T>`, a value in tag 24's byte string, encoded by counting then streaming it,
//! and decoded by a decoder of its own under the outer profile.

mod common;

use std::cell::Cell;
use std::mem::MaybeUninit;

use cbor_static::{Cbor, Decode, Decoder, Embedded, Encoder, ErrorKind};
use common::{decode, decode_strict, encode, golden_contiguous};

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Claims {
    sub: u8,
    admin: bool,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Signed {
    payload: Embedded<Claims>,
    #[cbor(bytes)]
    signature: [u8; 2],
}

#[test]
fn an_embedded_value_is_its_encoding_in_a_tagged_byte_string() {
    golden_contiguous(
        &Embedded(Claims {
            sub: 7,
            admin: false,
        }),
        b"\xd8\x18\x4d\xa2\x63sub\x07\x65admin\xf4",
    );
    golden_contiguous(
        &Signed {
            payload: Embedded(Claims {
                sub: 1,
                admin: true,
            }),
            signature: [1, 2],
        },
        b"\xa2\x67payload\xd8\x18\x4d\xa2\x63sub\x01\x65admin\xf5\x69signature\x42\x01\x02",
    );
    golden_contiguous(
        &Embedded(Embedded(500u16)),
        b"\xd8\x18\x46\xd8\x18\x43\x19\x01\xf4",
    );
    golden_contiguous(&Some(Embedded('a')), b"\xd8\x18\x42\x61a");
    golden_contiguous(&None::<Embedded<u8>>, b"\xf6");
}

#[test]
fn the_byte_string_must_hold_exactly_one_item() {
    assert_eq!(
        decode::<Embedded<u8>>(b"\xd8\x18\x42\x01\x02"),
        Err(ErrorKind::TrailingInput)
    );
    assert_eq!(
        decode::<Embedded<(u8, u8)>>(b"\xd8\x18\x42\x82\x01"),
        Err(ErrorKind::UnexpectedEnd)
    );
    assert_eq!(
        decode::<Embedded<u8>>(b"\xd8\x18\x40"),
        Err(ErrorKind::UnexpectedEnd)
    );
}

#[test]
fn only_tag_24_over_a_definite_byte_string_is_embedded() {
    assert_eq!(
        decode::<Embedded<u8>>(b"\xd8\x19\x41\x01"),
        Err(ErrorKind::UnexpectedTag)
    );
    assert_eq!(
        decode::<Embedded<u8>>(b"\x41\x01"),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(
        decode::<Embedded<u8>>(b"\xd8\x18\x61\x01"),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(
        decode::<Embedded<u8>>(b"\xd8\x18\x5f\x41\x01\xff"),
        Err(ErrorKind::IndefiniteString)
    );
}

#[test]
fn strict_reaches_the_embedded_value() {
    let preferred = b"\xd8\x18\x41\x01";
    let wide = b"\xd8\x18\x42\x18\x01";
    let unknown = b"\xd8\x18\x50\xa3\x63sub\x07\x65admin\xf4\x61z\x00";
    assert_eq!(decode_strict::<Embedded<u8>>(preferred), Ok(Embedded(1)));
    assert_eq!(decode::<Embedded<u8>>(wide), Ok(Embedded(1)));
    assert_eq!(
        decode_strict::<Embedded<u8>>(wide),
        Err(ErrorKind::NotDeterministic)
    );
    assert_eq!(
        decode::<Embedded<Claims>>(unknown),
        Ok(Embedded(Claims {
            sub: 7,
            admin: false
        }))
    );
    assert_eq!(
        decode_strict::<Embedded<Claims>>(unknown),
        Err(ErrorKind::UnknownField)
    );
    assert_eq!(
        decode_strict::<Embedded<u8>>(b"\xd8\x18\x58\x01\x01"),
        Err(ErrorKind::NotDeterministic),
        "the byte string's own head"
    );
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[cbor(deterministic)]
struct Root {
    value: Embedded<f64>,
}

/// The value is counted under the root's deterministic encoding, which writes a NaN's payload
/// away, so the byte string's head holds the length actually written (D-11b).
#[test]
fn an_embedded_value_is_counted_as_its_root_writes_it() {
    let nan = f64::from_bits(0x7ff8_0000_0000_0001);
    assert_eq!(
        encode(&Root {
            value: Embedded(nan)
        }),
        b"\xa1\x65value\xd8\x18\x43\xf9\x7e\x00"
    );
    assert_eq!(
        encode(&Embedded(nan)),
        b"\xd8\x18\x49\xfb\x7f\xf8\x00\x00\x00\x00\x00\x01"
    );
}

#[derive(cbor_static::Encode)]
struct Name<'a> {
    #[cbor(max_len = 2)]
    name: &'a str,
}

#[test]
fn an_embedded_value_that_fails_to_count_stops_the_encoder() {
    let embedded = Embedded(Name { name: "abc" });
    let mut encoder = Encoder::new(&embedded);
    for _ in 0..2 {
        let mut out = [0u8; 16];
        assert_eq!(
            encoder.resume(&mut out).map_err(|e| (e.kind, e.offset)),
            Err((ErrorKind::LimitExceeded, 0))
        );
        assert_eq!(out, [0; 16]);
    }
}

#[test]
fn an_embedded_value_takes_contiguous_input_and_no_frame_to_decode() {
    const { assert!(<Embedded<u8> as Decode>::CONTIGUOUS) };
    const { assert!(<Embedded<Claims> as Decode>::MAX_DEPTH == 0) };
    const { assert!(<Embedded<Claims> as cbor_static::Encode>::MAX_DEPTH == 2) };
}

thread_local! {
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Cbor, Debug, PartialEq)]
struct Tracked {
    id: u8,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.set(DROPS.get() + 1);
    }
}

#[derive(Cbor, Debug, PartialEq)]
struct Pair {
    first: Tracked,
    second: Tracked,
}

#[derive(Cbor, Debug, PartialEq)]
struct Holder {
    pair: Embedded<Pair>,
    last: bool,
}

/// Decodes a `Holder` and drops what it decoded, returning the outcome and the drops counted.
fn drops(input: &[u8]) -> (Result<(), ErrorKind>, usize) {
    DROPS.set(0);
    let mut slot = MaybeUninit::<Holder>::uninit();
    let decoded = {
        let mut decoder = Decoder::new(&mut slot);
        decoder
            .feed(input)
            .and_then(|_| decoder.finish().map(|holder| drop(taken(holder))))
            .map_err(|e| e.kind)
    };
    (decoded, DROPS.get())
}

/// Moves the decoded value out of its slot, leaving a placeholder there that is never dropped.
fn taken(holder: &mut Holder) -> Holder {
    std::mem::replace(
        holder,
        Holder {
            pair: Embedded(Pair {
                first: Tracked { id: 0 },
                second: Tracked { id: 0 },
            }),
            last: false,
        },
    )
}

const PAIR: &[u8] = b"\xa2\x65first\xa1\x62id\x01\x66second\xa1\x62id\x02";

fn holder(pair: &[u8], last: &[u8]) -> Vec<u8> {
    [
        b"\xa2\x64pair\xd8\x18".as_slice(),
        &[0x58, u8::try_from(pair.len()).expect("short")],
        pair,
        b"\x64last",
        last,
    ]
    .concat()
}

#[test]
fn an_embedded_value_drops_exactly_what_was_decoded_wherever_decoding_stops() {
    assert_eq!(drops(&holder(PAIR, b"\xf5")), (Ok(()), 2));
    assert_eq!(
        drops(&holder(PAIR, b"\x00")),
        (Err(ErrorKind::TypeMismatch), 2),
        "a later sibling fails"
    );
    assert_eq!(
        drops(&holder(&PAIR[..PAIR.len() - 1], b"\xf5")),
        (Err(ErrorKind::UnexpectedEnd), 1),
        "the embedded value fails partway"
    );
    assert_eq!(
        drops(&holder(&[PAIR, b"\x00"].concat(), b"\xf5")),
        (Err(ErrorKind::TrailingInput), 2),
        "the embedded value is whole, with bytes after it"
    );
}

#[test]
fn an_embedded_value_may_borrow_from_the_input() {
    let input = b"\xd8\x18\x44\x63abc";
    let mut slot = MaybeUninit::uninit();
    let text = cbor_static::decode_into::<Embedded<&str>>(input, &mut slot).expect("decodes");
    assert_eq!(text.0, "abc");
    assert!(input.as_ptr_range().contains(&text.0.as_ptr()));
}
