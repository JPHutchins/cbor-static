//! #5 Q11: typed items handed off in the middle of a token stream, by `Tokens::decode_into` and
//! `TokenWriter::encode`, and encoders and decoders that live in a `static` or move between
//! threads.

use std::mem::MaybeUninit;
use std::sync::Mutex;

use cbor_static::{
    Cbor, Decoder, Encoder, ErrorKind, Progress, Token, TokenWriter, Tokens, to_slice,
};
use proptest::prelude::*;

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 512 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn kinds(tokens: Tokens<'_>) -> Vec<Result<Token<'_>, ErrorKind>> {
    tokens.map(|token| token.map_err(|e| e.kind)).collect()
}

fn decoded<'de, T: cbor_static::Decode<'de> + Copy>(
    tokens: &mut Tokens<'de>,
) -> Result<T, (ErrorKind, u32)> {
    tokens
        .decode_into::<T>(&mut MaybeUninit::uninit())
        .map(|value| *value)
        .map_err(|e| (e.kind, e.offset))
}

fn sequence(points: &[Point]) -> Vec<u8> {
    points
        .iter()
        .flat_map(|point| {
            let mut out = [0u8; 32];
            let len = to_slice(point, &mut out).expect("a point fits");
            out[..len].to_vec()
        })
        .collect()
}

#[test]
fn a_typed_item_is_one_element_of_the_container_around_it() {
    let input = b"\x83\xa2\x61x\x01\x61y\x02\x07\xa2\x61x\x03\x61y\x04\x00";
    let mut tokens = Tokens::new(input);
    assert_eq!(tokens.next(), Some(Ok(Token::Array(Some(3)))));
    assert_eq!(decoded::<Point>(&mut tokens), Ok(Point { x: 1, y: 2 }));
    assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(7))));
    assert_eq!(decoded::<Point>(&mut tokens), Ok(Point { x: 3, y: 4 }));
    assert_eq!(kinds(tokens), [Ok(Token::Unsigned(0))]);
}

#[test]
fn a_typed_item_is_a_key_or_a_value_of_an_indefinite_map() {
    let input = b"\xbf\x82\x01\x02\xf5\x01\xa2\x61x\x05\x61y\x06\xff";
    let mut tokens = Tokens::new(input);
    assert_eq!(tokens.next(), Some(Ok(Token::Map(None))));
    assert_eq!(decoded::<(u8, u8)>(&mut tokens), Ok((1, 2)));
    assert_eq!(tokens.next(), Some(Ok(Token::Bool(true))));
    assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(1))));
    assert_eq!(decoded::<Point>(&mut tokens), Ok(Point { x: 5, y: 6 }));
    assert_eq!(kinds(tokens), [Ok(Token::Break)]);
}

#[test]
fn a_typed_key_still_owes_its_value_before_the_break() {
    let mut tokens = Tokens::new(b"\xbf\x01\xff");
    assert_eq!(tokens.next(), Some(Ok(Token::Map(None))));
    assert_eq!(decoded::<u8>(&mut tokens), Ok(1));
    assert_eq!(
        kinds(tokens),
        [Err(ErrorKind::NotWellFormed)],
        "D-14b: a key's value is owed"
    );
}

#[test]
fn no_item_begins_inside_an_indefinite_string_at_a_break_or_past_the_end() {
    let mut tokens = Tokens::new(b"\x5f\x41\x01\xff");
    assert_eq!(tokens.next(), Some(Ok(Token::BytesIndefinite)));
    assert_eq!(
        decoded::<[u8; 1]>(&mut tokens),
        Err((ErrorKind::NotWellFormed, 1))
    );
    assert_eq!(tokens.next(), Some(Ok(Token::Bytes(b"\x01"))));

    let mut tokens = Tokens::new(b"\x9f\xff");
    assert_eq!(tokens.next(), Some(Ok(Token::Array(None))));
    assert_eq!(
        decoded::<u8>(&mut tokens),
        Err((ErrorKind::NotWellFormed, 2))
    );
    assert_eq!(kinds(tokens), [Ok(Token::Break)]);

    let mut tokens = Tokens::new(b"\x01");
    assert_eq!(decoded::<u8>(&mut tokens), Ok(1));
    assert_eq!(
        decoded::<u8>(&mut tokens),
        Err((ErrorKind::UnexpectedEnd, 1))
    );
    assert_eq!(tokens.next(), None);
}

#[test]
fn a_failed_decode_leaves_the_tokens_to_retry_as_another_type() {
    let mut tokens = Tokens::new(b"\x82\x01\x02\x03");
    assert_eq!(
        decoded::<Point>(&mut tokens),
        Err((ErrorKind::TypeMismatch, 1))
    );
    assert_eq!(decoded::<(u8, u8)>(&mut tokens), Ok((1, 2)));
    assert_eq!(
        decoded::<bool>(&mut tokens),
        Err((ErrorKind::TypeMismatch, 4))
    );
    assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(3))));
    assert_eq!(tokens.next(), None);
}

#[test]
fn a_failed_decode_leaves_the_first_token_of_its_item_next() {
    let mut tokens = Tokens::new(b"\xa1\x61x\x01");
    assert_eq!(
        decoded::<Point>(&mut tokens).map_err(|e| e.0),
        Err(ErrorKind::MissingField)
    );
    assert_eq!(
        kinds(tokens),
        [
            Ok(Token::Map(Some(1))),
            Ok(Token::Text("x")),
            Ok(Token::Unsigned(1))
        ]
    );
}

#[test]
fn malformed_input_fails_alike_on_every_retry() {
    let mut tokens = Tokens::new(b"\x82\x01\xff");
    for _ in 0..3 {
        assert_eq!(
            decoded::<(u8, u8)>(&mut tokens),
            Err((ErrorKind::NotWellFormed, 3))
        );
    }
    assert_eq!(tokens.next(), Some(Ok(Token::Array(Some(2)))));
}

#[test]
fn every_truncation_is_unexpected_end_and_moves_nothing() {
    let input = sequence(&[Point { x: 1, y: -300 }]);
    for len in 0..input.len() {
        let mut tokens = Tokens::new(&input[..len]);
        assert_eq!(
            decoded::<Point>(&mut tokens).map_err(|e| e.0),
            Err(ErrorKind::UnexpectedEnd),
            "{len}"
        );
        assert_eq!(kinds(tokens), kinds(Tokens::new(&input[..len])), "{len}");
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn typed_items_leave_the_tokens_of_the_items_after_them(
        points in prop::collection::vec(any::<(i32, i32)>(), 0..6),
        start in 0usize..6,
    ) {
        let points: Vec<Point> = points.into_iter().map(|(x, y)| Point { x, y }).collect();
        let input = sequence(&points);
        for at in 0..=points.len() {
            let mut tokens = Tokens::new(&input);
            for (index, point) in points.iter().enumerate().take(at) {
                if index == start {
                    prop_assert_eq!(tokens.next(), Some(Ok(Token::Map(Some(2)))));
                    for _ in 0..4 {
                        prop_assert!(tokens.next().is_some_and(|token| token.is_ok()));
                    }
                } else {
                    prop_assert_eq!(decoded::<Point>(&mut tokens), Ok(*point));
                }
            }
            prop_assert_eq!(
                kinds(tokens),
                kinds(Tokens::new(&input[sequence(&points[..at]).len()..]))
            );
        }
    }
}

#[test]
fn an_encoded_item_follows_the_tokens_before_it() {
    let mut out = [0u8; 16];
    let mut writer = TokenWriter::new(&mut out);
    writer
        .write(Token::Array(Some(2)))
        .and_then(|writer| writer.encode(&Point { x: 1, y: -1 }))
        .and_then(|writer| writer.write(Token::Null))
        .expect("fits");
    let len = writer.written();
    assert_eq!(&out[..len], b"\x82\xa2\x61x\x01\x61y\x20\xf6");
}

#[test]
fn an_encoding_that_does_not_fit_leaves_the_written_length() {
    let mut out = [0u8; 7];
    let mut writer = TokenWriter::new(&mut out);
    writer.write(Token::Unsigned(1)).expect("fits");
    assert_eq!(
        writer
            .encode(&Point { x: 1, y: 2 })
            .map(|_| ())
            .map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::BufferTooSmall, 1))
    );
    assert_eq!(writer.written(), 1);
}

#[test]
fn an_encode_error_is_at_its_offset_into_the_buffer() {
    #[derive(cbor_static::Encode)]
    struct Name<'a> {
        #[cbor(max_len = 2)]
        name: &'a str,
    }

    let mut out = [0u8; 16];
    let mut writer = TokenWriter::new(&mut out);
    writer.write(Token::Unsigned(1)).expect("fits");
    assert_eq!(
        writer
            .encode(&Name { name: "abc" })
            .map(|_| ())
            .map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::LimitExceeded, 7))
    );
    assert_eq!(writer.written(), 1);
}

fn send(_: impl Send) {}

#[test]
fn encoders_and_decoders_of_thread_safe_values_are_send() {
    send(Encoder::new(&Point { x: 1, y: 2 }));
    send(Decoder::new(&mut MaybeUninit::<Point>::uninit()));
    send(Decoder::new(&mut MaybeUninit::<&str>::uninit()));
}

static READING: Point = Point { x: 500, y: -1 };
static ENCODER: Mutex<Option<Encoder<'static, Point>>> = Mutex::new(None);

#[test]
fn a_static_encoder_resumes_on_another_thread() {
    let mut first = [0u8; 4];
    {
        let mut guard = ENCODER.lock().expect("unpoisoned");
        let encoder = guard.insert(Encoder::new(&READING));
        assert_eq!(encoder.resume(&mut first), Ok(Progress::NeedSpace(4)));
    }
    let rest = std::thread::spawn(|| {
        let mut rest = [0u8; 16];
        let mut guard = ENCODER.lock().expect("unpoisoned");
        match guard.as_mut().map(|encoder| encoder.resume(&mut rest)) {
            Some(Ok(Progress::Done(len))) => rest[..len].to_vec(),
            other => panic!("{other:?}"),
        }
    })
    .join()
    .expect("the thread finishes");
    assert_eq!(
        [first.as_slice(), &rest].concat(),
        b"\xa2\x61x\x19\x01\xf4\x61y\x20"
    );
}
