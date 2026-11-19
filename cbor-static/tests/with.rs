//! #5 Q19: `#[cbor(with = module)]`, a field its module writes and reads as exactly one item through
//! a `TokenWriter` and `Tokens`.
#![expect(
    clippy::trivially_copy_pass_by_ref,
    clippy::ref_option,
    clippy::unnecessary_wraps,
    reason = "a module's `encode` and `decode` take the signatures #5 Q19 gives them"
)]

mod common;

use std::cell::Cell;
use std::mem::MaybeUninit;

use cbor_static::{Cbor, Decode, Decoder, Encode, Encoder, ErrorKind};
use common::{decode, decode_strict, encode, golden_contiguous};

/// A pair of bytes, written as one byte string.
mod pair {
    use cbor_static::{Error, ErrorKind, Token, TokenWriter, Tokens};

    pub fn encode(value: &(u8, u8), writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.write(Token::Bytes(&[value.0, value.1])).map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<(u8, u8), Error> {
        match tokens.next() {
            Some(Ok(Token::Bytes(&[a, b]))) => Ok((a, b)),
            Some(Err(error)) => Err(error),
            _ => Err(ErrorKind::TypeMismatch.into()),
        }
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Reading {
    id: u8,
    #[cbor(with = pair)]
    raw: (u8, u8),
    ok: bool,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Positional(#[cbor(with = pair)] (u8, u8), u8);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Newtype(#[cbor(with = pair)] (u8, u8));

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
enum Frame {
    Raw(#[cbor(with = pair)] (u8, u8)),
    Tagged {
        #[cbor(with = pair)]
        raw: (u8, u8),
    },
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Generic<T> {
    #[cbor(with = pair)]
    raw: (u8, u8),
    value: T,
}

#[test]
fn a_module_writes_and_reads_its_field_in_every_form() {
    golden_contiguous(
        &Reading {
            id: 1,
            raw: (2, 3),
            ok: true,
        },
        b"\xa3\x62id\x01\x63raw\x42\x02\x03\x62ok\xf5",
    );
    golden_contiguous(&Positional((4, 5), 6), b"\x82\x42\x04\x05\x06");
    golden_contiguous(&Newtype((7, 8)), b"\x42\x07\x08");
    golden_contiguous(&Frame::Raw((1, 2)), b"\xa1\x63Raw\x42\x01\x02");
    golden_contiguous(
        &Frame::Tagged { raw: (3, 4) },
        b"\xa1\x66Tagged\xa1\x63raw\x42\x03\x04",
    );
    golden_contiguous(
        &Generic {
            raw: (5, 6),
            value: -1i8,
        },
        b"\xa2\x63raw\x42\x05\x06\x65value\x20",
    );
}

#[test]
fn a_module_error_is_the_fields_error() {
    assert_eq!(
        cbor_static::decode_into::<Reading>(
            b"\xa3\x62id\x01\x63raw\x41\x02\x62ok\xf5",
            &mut MaybeUninit::uninit()
        )
        .map(|_| ())
        .map_err(|e| (e.kind, e.field, e.offset)),
        Err((ErrorKind::TypeMismatch, 1, 10))
    );
    for (how, kind) in [(3, ErrorKind::OutOfRange), (4, ErrorKind::BufferTooSmall)] {
        let wrong = Wrong { how };
        let mut encoder = Encoder::new(&wrong);
        for _ in 0..2 {
            assert_eq!(
                encoder
                    .resume(&mut [0u8; 16])
                    .map_err(|e| (e.kind, e.field, e.offset)),
                Err((kind, 0, 5)),
                "a module's own error stops the encoding, BufferTooSmall included"
            );
        }
    }
}

#[test]
fn a_with_field_is_one_frame_to_encode_and_none_to_decode() {
    const { assert!(<Newtype as Encode>::MAX_DEPTH == 1) };
    const { assert!(<Newtype as Decode>::MAX_DEPTH == 0) };
    const { assert!(<Reading as Encode>::MAX_DEPTH == 2) };
    const { assert!(<Reading as Decode>::MAX_DEPTH == 1) };
}

/// Writes nothing, two items, or an array missing an element, as `HOW` says.
mod wrong {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(how: &u8, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        match how {
            0 => Ok(()),
            3 => Err(cbor_static::ErrorKind::OutOfRange.into()),
            4 => Err(cbor_static::ErrorKind::BufferTooSmall.into()),
            2 => writer
                .write(Token::Unsigned(1))
                .and_then(|writer| writer.write(Token::Unsigned(2)))
                .map(|_| ()),
            _ => writer
                .write(Token::Array(Some(2)))
                .and_then(|writer| writer.encode(&1u8))
                .map(|_| ()),
        }
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<u8, Error> {
        tokens.next().map_or(Ok(0), |token| token.map(|_| 1))
    }
}

/// Writes its value, and reads nothing.
mod idle {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(value: &u8, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.write(Token::Unsigned((*value).into())).map(|_| ())
    }

    pub fn decode(_: &mut Tokens<'_>) -> Result<u8, Error> {
        Ok(0)
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Idle(#[cbor(with = idle)] u8);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Wrong {
    #[cbor(with = wrong)]
    how: u8,
}

#[test]
fn a_module_that_writes_other_than_one_item_stops_the_encoder() {
    for wrong in [0, 2, 1].map(|how| Wrong { how }) {
        let mut encoder = Encoder::new(&wrong);
        for _ in 0..3 {
            let mut out = [0u8; 16];
            assert_eq!(
                encoder.resume(&mut out).map_err(|e| (e.kind, e.offset)),
                Err((ErrorKind::NotWellFormed, 5)),
                "{wrong:?}"
            );
        }
    }
}

#[test]
fn a_module_that_leaves_tokens_unread_is_trailing_input() {
    assert_eq!(
        decode::<Idle>(b"\x00"),
        Err(ErrorKind::TrailingInput),
        "none read"
    );
    assert_eq!(
        decode::<Wrong>(b"\xa1\x63how\x82\x01\x02"),
        Err(ErrorKind::TrailingInput),
        "one of three read"
    );
    assert_eq!(decode::<Wrong>(b"\xa1\x63how\x00"), Ok(Wrong { how: 1 }));
}

thread_local! {
    static RUNS: Cell<usize> = const { Cell::new(0) };
    static SHIFT: Cell<u8> = const { Cell::new(0) };
}

/// A byte string of `len` bytes, counting each run.
mod counted {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(len: &usize, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        super::RUNS.set(super::RUNS.get() + 1);
        writer.write(Token::Bytes(&[7u8; 256][..*len])).map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<usize, Error> {
        match tokens.next() {
            Some(Ok(Token::Bytes(bytes))) => Ok(bytes.len()),
            _ => Ok(usize::MAX),
        }
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Counted(#[cbor(with = counted)] usize);

#[test]
fn a_module_runs_once_to_open_and_once_per_eighteen_bytes() {
    for (len, written, runs) in [(0, 1, 2), (17, 18, 2), (18, 19, 3), (198, 200, 13)] {
        RUNS.set(0);
        let bytes = encode(&Counted(len));
        assert_eq!(bytes.len(), written, "{len}");
        assert_eq!(RUNS.get(), runs, "{len}");
        assert_eq!(decode::<Counted>(&bytes), Ok(Counted(len)));
    }
}

#[test]
fn a_long_item_streams_into_any_chunk() {
    let bytes = encode(&Counted(199));
    for chunk in 1..=64 {
        assert_eq!(
            common::chunked_encode(&Counted(199), chunk),
            bytes,
            "{chunk}"
        );
    }
}

/// A byte string whose length grows by one each run, as no module should.
mod shifting {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(_: &u8, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        let shift = super::SHIFT.get();
        super::SHIFT.set(shift + 1);
        writer
            .write(Token::Bytes(&[0u8; 64][..20 + usize::from(shift)]))
            .map(|_| ())
    }

    pub fn decode(_: &mut Tokens<'_>) -> Result<u8, Error> {
        Ok(0)
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Shifting(#[cbor(with = shifting)] u8);

#[test]
fn a_module_whose_runs_differ_is_not_well_formed_from_then_on() {
    SHIFT.set(0);
    let mut encoder = Encoder::new(&Shifting(0));
    let mut out = [0u8; 64];
    assert_eq!(
        encoder.resume(&mut out).map_err(|e| e.kind),
        Err(ErrorKind::NotWellFormed)
    );
    SHIFT.set(0);
    assert_eq!(
        encoder.resume(&mut out).map_err(|e| e.kind),
        Err(ErrorKind::NotWellFormed)
    );
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Inner {
    x: u8,
}

/// An `Inner`, decoded through `Tokens::decode_into`, and encoded through `TokenWriter::encode`.
mod typed {
    use core::mem::MaybeUninit;

    use cbor_static::{Error, TokenWriter, Tokens};

    pub fn encode(value: &super::Inner, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.encode(value).map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<super::Inner, Error> {
        tokens
            .decode_into(&mut MaybeUninit::uninit())
            .map(|inner| *inner)
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Outer {
    #[cbor(with = typed)]
    inner: Inner,
}

#[test]
fn a_module_decodes_its_typed_items_under_the_decoders_profile() {
    let known = b"\xa1\x65inner\xa1\x61x\x01";
    let unknown = b"\xa1\x65inner\xa2\x61x\x01\x61y\x02";
    assert_eq!(
        decode::<Outer>(known),
        Ok(Outer {
            inner: Inner { x: 1 }
        })
    );
    assert_eq!(
        decode_strict::<Outer>(known),
        Ok(Outer {
            inner: Inner { x: 1 }
        })
    );
    assert_eq!(
        decode::<Outer>(unknown),
        Ok(Outer {
            inner: Inner { x: 1 }
        })
    );
    assert_eq!(
        decode_strict::<Outer>(unknown),
        Err(ErrorKind::UnknownField)
    );
    assert_eq!(
        decode_strict::<Outer>(b"\xa1\x65inner\xa1\x61x\x18\x01"),
        Err(ErrorKind::NotDeterministic)
    );
}

#[test]
fn a_typed_item_encodes_through_a_window() {
    golden_contiguous(
        &Outer {
            inner: Inner { x: 1 },
        },
        b"\xa1\x65inner\xa1\x61x\x01",
    );
}

#[derive(Cbor, Debug, PartialEq)]
struct Missing {
    #[serde(default)]
    #[cbor(with = maybe)]
    value: Option<u8>,
    id: u8,
}

/// An `Option<u8>` as its value or null.
mod maybe {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(value: &Option<u8>, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer
            .write(value.map_or(Token::Null, |value| Token::Unsigned(value.into())))
            .map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<Option<u8>, Error> {
        match tokens.next() {
            Some(Ok(Token::Unsigned(value))) => Ok(u8::try_from(value).ok()),
            _ => Ok(None),
        }
    }
}

#[test]
fn a_missing_with_field_takes_its_default() {
    let mut slot = MaybeUninit::uninit();
    assert_eq!(
        cbor_static::decode_into::<Missing>(b"\xa1\x62id\x07", &mut slot).map_err(|e| e.kind),
        Ok(&mut Missing { value: None, id: 7 })
    );
    assert_eq!(
        encode(&Missing {
            value: Some(1),
            id: 7
        }),
        b"\xa2\x65value\x01\x62id\x07"
    );
}

thread_local! {
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug, PartialEq)]
struct Tracked(u8);

impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.set(DROPS.get() + 1);
    }
}

/// A `Tracked`, read from an unsigned integer; 255 panics.
mod tracked {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(value: &super::Tracked, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.write(Token::Unsigned(value.0.into())).map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<super::Tracked, Error> {
        match tokens.next() {
            Some(Ok(Token::Unsigned(255))) => panic!("the module fails"),
            Some(Ok(Token::Unsigned(value))) => {
                Ok(super::Tracked(u8::try_from(value).unwrap_or(0)))
            }
            _ => Ok(super::Tracked(0)),
        }
    }
}

#[derive(Cbor, Debug, PartialEq)]
struct Holder {
    #[cbor(with = tracked)]
    first: Tracked,
    #[cbor(with = tracked)]
    second: Tracked,
    last: bool,
}

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

/// Moves the decoded value out of its slot, which a decoder that finished has handed over, leaving
/// one that holds nothing to drop.
fn taken(holder: &mut Holder) -> Holder {
    std::mem::replace(
        holder,
        Holder {
            first: Tracked(0),
            second: Tracked(0),
            last: false,
        },
    )
}

#[test]
fn a_decoded_module_value_is_dropped_once_wherever_decoding_stops() {
    assert_eq!(
        drops(b"\xa3\x65first\x01\x66second\x02\x64last\xf5"),
        (Ok(()), 2)
    );
    assert_eq!(
        drops(b"\xa3\x65first\x01\x66second\x02\x64last\x00"),
        (Err(ErrorKind::TypeMismatch), 2),
        "a later sibling fails"
    );
    assert_eq!(
        drops(b"\xa3\x65first\x01\x66second\x82\x01\x02\x64last\xf5"),
        (Err(ErrorKind::TrailingInput), 2),
        "the second's module leaves its item unread: its value and the first drop"
    );
}

#[test]
fn a_module_that_panics_drops_what_was_decoded_before_it() {
    DROPS.set(0);
    let unwound =
        std::panic::catch_unwind(|| drops(b"\xa3\x65first\x01\x66second\x18\xff\x64last\xf5"));
    assert!(unwound.is_err());
    assert_eq!(DROPS.get(), 1);
}

#[test]
fn a_modules_own_buffer_too_small_is_what_encoded_len_returns() {
    assert_eq!(
        cbor_static::encoded_len(&Wrong { how: 4 }).map_err(|e| e.kind),
        Err(ErrorKind::BufferTooSmall)
    );
}

#[derive(cbor_static::Encode)]
struct Name<'a> {
    #[cbor(max_len = 2)]
    name: &'a str,
}

/// Tries to encode a name, past `long` bytes of text, and writes 7 in its place when it fails.
mod fallback {
    use cbor_static::{Error, Token, TokenWriter, Tokens};

    pub fn encode(long: &usize, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        let text = "x".repeat(*long);
        match writer.encode(&(text.as_str(), super::Name { name: "abc" })) {
            Ok(_) => Ok(()),
            Err(_) => writer.write(Token::Unsigned(7)).map(|_| ()),
        }
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<usize, Error> {
        tokens.next().map_or(Ok(0), |token| token.map(|_| 0))
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Fallback(#[cbor(with = fallback)] usize);

/// A typed encode that fails, before or after spilling past the 64 B it encodes through, leaves
/// the window's item as it was, as it leaves a plain writer's.
#[test]
fn a_failed_typed_encode_in_a_window_leaves_the_item_owed() {
    for long in [1, 100] {
        assert_eq!(encode(&Fallback(long)), b"\x07", "{long}");
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Raw(#[cbor(with = pair)] (u8, u8)),
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[cbor(index)]
enum Indexed {
    Raw(#[cbor(with = pair)] (u8, u8)),
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
enum CborTagged {
    #[cbor(cbor_tag = 40)]
    Raw(#[cbor(with = pair)] (u8, u8)),
}

#[test]
fn a_with_newtype_payload_is_written_in_every_enum_form_that_takes_one_as_it_is() {
    golden_contiguous(&Adjacent::Raw((1, 2)), b"\xa2\x61t\x63Raw\x61c\x42\x01\x02");
    golden_contiguous(&Indexed::Raw((1, 2)), b"\x82\x00\x42\x01\x02");
    golden_contiguous(&CborTagged::Raw((1, 2)), b"\xd8\x28\x42\x01\x02");
}

/// A text borrowed from the input.
mod borrowed {
    use cbor_static::{Error, ErrorKind, Token, TokenWriter, Tokens};

    pub fn encode(value: &&str, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.write(Token::Text(value)).map(|_| ())
    }

    pub fn decode<'de>(tokens: &mut Tokens<'de>) -> Result<&'de str, Error> {
        match tokens.next() {
            Some(Ok(Token::Text(text))) => Ok(text),
            _ => Err(ErrorKind::TypeMismatch.into()),
        }
    }
}

#[derive(Cbor, Debug, PartialEq)]
struct Named<'a> {
    #[cbor(with = borrowed)]
    name: &'a str,
}

#[test]
fn a_module_may_borrow_from_the_input() {
    let input = b"\xa1\x64name\x63abc";
    let mut slot = MaybeUninit::uninit();
    let named = cbor_static::decode_into::<Named>(input, &mut slot).expect("decodes");
    assert_eq!(named.name, "abc");
    assert!(input.as_ptr_range().contains(&named.name.as_ptr()));
    assert_eq!(encode(&Named { name: "abc" }), input);
}

/// Any typed value, handed to and from the engine.
mod generic {
    use core::mem::MaybeUninit;

    use cbor_static::{Decode, Encode, Error, TokenWriter, Tokens};

    pub fn encode<T: Encode>(value: &T, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.encode(value).map(|_| ())
    }

    pub fn decode<'de, T: Decode<'de> + Copy>(tokens: &mut Tokens<'de>) -> Result<T, Error> {
        tokens
            .decode_into(&mut MaybeUninit::uninit())
            .map(|value| *value)
    }
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Holds<T: Copy> {
    #[cbor(with = generic)]
    value: T,
}

#[test]
fn a_with_field_may_be_of_a_type_parameter() {
    golden_contiguous(&Holds { value: 500u16 }, b"\xa1\x65value\x19\x01\xf4");
    golden_contiguous(
        &Holds {
            value: (true, -1i8),
        },
        b"\xa1\x65value\x82\xf5\x20",
    );
}
