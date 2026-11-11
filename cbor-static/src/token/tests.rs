use std::vec;
use std::vec::Vec;

use super::{Token, TokenWriter, Tokens};
use crate::error::ErrorKind;

fn tokens(bytes: &[u8]) -> Result<Vec<Token<'_>>, (ErrorKind, u32)> {
    Tokens::new(bytes)
        .collect::<Result<_, _>>()
        .map_err(|e| (e.kind, e.offset))
}

fn reencoded(bytes: &[u8]) -> Result<Vec<u8>, ErrorKind> {
    let mut out = [0u8; 64];
    let mut writer = TokenWriter::new(&mut out);
    for token in tokens(bytes).map_err(|(kind, _)| kind)? {
        writer.write(token).map_err(|e| e.kind)?;
    }
    let len = writer.written();
    Ok(out[..len].to_vec())
}

#[test]
fn every_head_becomes_its_token() {
    let cases: [(&[u8], Token); 20] = [
        (b"\x00", Token::Unsigned(0)),
        (
            b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff",
            Token::Unsigned(u64::MAX),
        ),
        (b"\x20", Token::Negative(0)),
        (
            b"\x3b\xff\xff\xff\xff\xff\xff\xff\xff",
            Token::Negative(u64::MAX),
        ),
        (b"\x42\x01\x02", Token::Bytes(&[1, 2])),
        (b"\x62ab", Token::Text("ab")),
        (b"\x80", Token::Array(Some(0))),
        (b"\x9a\x00\x00\x00\x00", Token::Array(Some(0))),
        (b"\xa0", Token::Map(Some(0))),
        (b"\xe0", Token::Simple(0)),
        (b"\xf3", Token::Simple(19)),
        (b"\xf8\x20", Token::Simple(32)),
        (b"\xf8\xff", Token::Simple(255)),
        (b"\xf4", Token::Bool(false)),
        (b"\xf5", Token::Bool(true)),
        (b"\xf6", Token::Null),
        (b"\xf7", Token::Undefined),
        (b"\xf9\x3c\x00", Token::Float(1.0)),
        (b"\xfa\x47\xc3\x50\x00", Token::Float(100_000.0)),
        (b"\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a", Token::Float(1.1)),
    ];
    for (bytes, token) in cases {
        assert_eq!(tokens(bytes), Ok(vec![token]), "{bytes:x?}");
    }
}

#[test]
fn containers_tags_and_chunks_are_flat_streams() {
    assert_eq!(
        tokens(b"\x5f\x41\x01\xff\xbf\x61a\x01\xff\xd9\xd9\xf7\x00\x01"),
        Ok(vec![
            Token::BytesIndefinite,
            Token::Bytes(&[1]),
            Token::Break,
            Token::Map(None),
            Token::Text("a"),
            Token::Unsigned(1),
            Token::Break,
            Token::Tag(55_799),
            Token::Unsigned(0),
            Token::Unsigned(1),
        ])
    );
}

#[test]
fn malformed_input_fails_at_the_offending_head() {
    let cases: [(&[u8], (ErrorKind, u32)); 9] = [
        (b"\x82\x01", (ErrorKind::UnexpectedEnd, 2)),
        (b"\x19\x01", (ErrorKind::UnexpectedEnd, 0)),
        (b"\x43\x01\x02", (ErrorKind::UnexpectedEnd, 0)),
        (b"\xff", (ErrorKind::NotWellFormed, 0)),
        (b"\x1c", (ErrorKind::NotWellFormed, 0)),
        (b"\xf8\x1f", (ErrorKind::NotWellFormed, 0)),
        (b"\x5f\x01\xff", (ErrorKind::NotWellFormed, 1)),
        (b"\x9f\x82\x01\xff\x02", (ErrorKind::NotWellFormed, 3)),
        (b"\x7f\x61\xc3\xff", (ErrorKind::InvalidUtf8, 1)),
    ];
    for (bytes, error) in cases {
        assert_eq!(tokens(bytes), Err(error), "{bytes:x?}");
    }
}

#[test]
fn an_error_ends_the_iteration() {
    let mut tokens = Tokens::new(b"\x01\xff\x01");
    assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(1))));
    assert!(matches!(tokens.next(), Some(Err(_))));
    assert_eq!(tokens.next(), None);
    assert_eq!(tokens.next(), None);
}

#[test]
fn the_writer_reencodes_preferred() {
    let cases: [(&[u8], &[u8]); 7] = [
        (b"\x18\x00", b"\x00"),
        (b"\x38\x00", b"\x20"),
        (b"\x19\x00\x18", b"\x18\x18"),
        (b"\x5a\x00\x00\x00\x01\x61", b"\x41\x61"),
        (b"\xfb\x3f\xf8\x00\x00\x00\x00\x00\x00", b"\xf9\x3e\x00"),
        (b"\xfa\x7f\xc0\x00\x00", b"\xf9\x7e\x00"),
        (b"\x1b\x00\x00\x00\x00\x00\x00\x01\x00", b"\x19\x01\x00"),
    ];
    for (input, preferred) in cases {
        assert_eq!(reencoded(input).as_deref(), Ok(preferred), "{input:x?}");
    }
    let definite = b"\x83\xc1\x1a\x51\x4b\x67\xb0\xa1\x61a\xf6\x82\xf8\x20\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a";
    assert_eq!(reencoded(definite).as_deref(), Ok(definite.as_slice()));
}

#[test]
fn the_writer_rejects_indefinite_tokens_and_writes_nothing_on_error() {
    for token in [
        Token::BytesIndefinite,
        Token::TextIndefinite,
        Token::Array(None),
        Token::Map(None),
        Token::Break,
        Token::Simple(20),
        Token::Simple(23),
    ] {
        assert_eq!(
            TokenWriter::new(&mut [0; 4])
                .write(token)
                .map(|_| ())
                .map_err(|e| e.kind),
            Err(ErrorKind::Unsupported),
            "{token:?}"
        );
    }
    let mut out = [0u8; 3];
    let mut writer = TokenWriter::new(&mut out);
    assert_eq!(
        writer
            .write(Token::Text("abc"))
            .map(|_| ())
            .map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::BufferTooSmall, 0))
    );
    assert_eq!(writer.written(), 0);
    assert_eq!(out, [0; 3]);
}
