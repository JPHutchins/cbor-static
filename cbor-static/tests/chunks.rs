#![forbid(unsafe_code)]

mod common;

use cbor_static::{Cbor, Chunks, ErrorKind, Tagged, encode_into};
use common::{chunked_decode, chunked_encode, decode, decode_strict, encode};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Note<'a> {
    text: Chunks<'a, str>,
    data: Option<Chunks<'a, [u8]>>,
}

/// The same note sent definite, in chunks (empty ones included), and with no chunks at all.
const NOTES: [&[u8]; 3] = [
    b"\xa2\x64text\x63abc\x64data\x42\x01\x02",
    b"\xa2\x64text\x7f\x61a\x60\x62bc\xff\x64data\x5f\x41\x01\x40\x41\x02\xff",
    b"\xa2\x64text\x7f\x63abc\xff\x64data\x5f\x42\x01\x02\xff",
];

#[test]
fn definite_and_chunked_strings_decode_to_their_chunks() {
    let [definite, chunked, whole] = NOTES.map(decode::<Note>).map(|note| note.expect("decodes"));
    assert!(definite.text.into_iter().eq(["abc"]));
    assert!(chunked.text.into_iter().eq(["a", "", "bc"]));
    assert!(chunked.data.is_some_and(|data| data.into_iter().eq([
        b"\x01".as_slice(),
        b"",
        b"\x02"
    ])));
    assert!(whole.text.into_iter().eq(["abc"]));
    assert_eq!([chunked, whole], [definite; 2], "equal however chunked");
    assert!(
        decode::<Chunks<str>>(b"\x7f\xff").is_ok_and(|text| text.into_iter().eq([] as [&str; 0]))
    );
    assert!(
        decode::<Chunks<[u8]>>(b"\x40").is_ok_and(|data| data.into_iter().eq([b"".as_slice()]))
    );
}

#[test]
fn chunks_encode_as_one_definite_string_at_every_output_size() {
    for bytes in NOTES {
        let note = decode::<Note>(bytes).expect("decodes");
        assert_eq!(encode(&note), NOTES[0], "{bytes:x?}");
        for chunk in 1..=NOTES[0].len() {
            assert_eq!(
                chunked_encode(&note, chunk),
                NOTES[0],
                "in chunks of {chunk}"
            );
        }
    }
    assert_eq!(
        encode(&decode::<Note>(b"\xa2\x64text\x7f\xff\x64data\xf6").expect("decodes")),
        b"\xa2\x64text\x60\x64data\xf6"
    );
}

#[test]
fn every_chunk_is_checked_as_it_is_decoded() {
    let cases: [(&[u8], ErrorKind); 6] = [
        (b"\x7f\x41a\xff", ErrorKind::NotWellFormed),
        (b"\x7f\x7f\xff\xff", ErrorKind::NotWellFormed),
        (b"\x7f\x61\xff\xff", ErrorKind::InvalidUtf8),
        (b"\x7f\x61\xc3\x61\xa9\xff", ErrorKind::InvalidUtf8),
        (b"\x7f\x61a", ErrorKind::UnexpectedEnd),
        (b"\x41a", ErrorKind::TypeMismatch),
    ];
    for (bytes, error) in cases {
        assert_eq!(decode::<Chunks<str>>(bytes), Err(error), "{bytes:x?}");
    }
    assert_eq!(
        decode::<Chunks<[u8]>>(b"\x5f\x61a\xff"),
        Err(ErrorKind::NotWellFormed)
    );
    assert_eq!(
        decode::<Chunks<str>>(b"\x7f\x62\xc3\xa9\xff").map(|text| text.into_iter().eq(["é"])),
        Ok(true)
    );
}

#[test]
fn an_indefinite_string_needs_contiguous_input() {
    let chunked = NOTES[1];
    for chunk in 1..chunked.len() {
        assert!(
            matches!(
                chunked_decode::<Note>(chunked, chunk),
                Err(ErrorKind::NeedContiguous)
            ) || chunked_decode::<Note>(chunked, chunk) == decode::<Note>(chunked),
            "in chunks of {chunk}"
        );
    }
    assert_eq!(
        chunked_decode::<Chunks<str>>(b"\x7f\x61a\xff", 2),
        Err(ErrorKind::NeedContiguous)
    );
    assert!(chunked_decode::<Note>(NOTES[0], 1).is_err_and(|e| e == ErrorKind::NeedContiguous));
}

#[test]
fn a_borrowed_string_rejects_an_indefinite_one() {
    for chunk in [1, 2, 64] {
        assert_eq!(
            chunked_decode::<&str>(b"\x7f\x61a\xff", chunk),
            Err(ErrorKind::IndefiniteString)
        );
        assert_eq!(
            chunked_decode::<&[u8]>(b"\x5f\x41a\xff", chunk),
            Err(ErrorKind::IndefiniteString)
        );
    }
    assert_eq!(decode::<&str>(b"\x5f\xff"), Err(ErrorKind::TypeMismatch));
}

#[test]
fn other_fields_reject_an_indefinite_string_by_its_type() {
    for bytes in [b"\x7f\x61a\xff".as_slice(), b"\x5f\x41a\xff"] {
        assert_eq!(
            decode::<u32>(bytes),
            Err(ErrorKind::TypeMismatch),
            "{bytes:x?}"
        );
        assert_eq!(
            decode::<f64>(bytes),
            Err(ErrorKind::TypeMismatch),
            "{bytes:x?}"
        );
        assert_eq!(
            decode::<bool>(bytes),
            Err(ErrorKind::TypeMismatch),
            "{bytes:x?}"
        );
        assert_eq!(
            decode::<Tagged<1, u32>>(bytes),
            Err(ErrorKind::UnexpectedTag),
            "{bytes:x?}"
        );
    }
}

#[test]
fn strict_takes_only_definite_chunks() {
    assert_eq!(
        decode_strict::<Note>(b"\xa2\x64data\x42\x01\x02\x64text\x63abc"),
        decode::<Note>(NOTES[0])
    );
    for bytes in [
        b"\xa2\x64data\x5f\x42\x01\x02\xff\x64text\x63abc".as_slice(),
        b"\xa2\x64data\x42\x01\x02\x64text\x7f\x63abc\xff",
    ] {
        assert_eq!(
            decode_strict::<Note>(bytes),
            Err(ErrorKind::NotDeterministic),
            "{bytes:x?}"
        );
    }
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
enum External<'a> {
    Text(Chunks<'a, str>),
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(tag = "t")]
enum Internal<'a> {
    Text(Chunks<'a, str>),
    Number(u32),
}

#[test]
fn chunks_are_a_variant_payload_but_never_a_map_to_merge() {
    let text = decode::<Chunks<str>>(b"\x7f\x62ab\x61c\xff").expect("decodes");
    assert_eq!(encode(&External::Text(text)), b"\xa1\x64Text\x63abc");
    let mut out = [0u8; 16];
    for value in [Internal::Text(text), Internal::Number(1)] {
        assert_eq!(
            encode_into(&value, &mut out).map_err(|e| e.kind),
            Err(ErrorKind::Unsupported),
            "{value:?}"
        );
    }
}
