//! Owned strings decode chunk by chunk from definite and indefinite strings (D-13, D-16, the half of
//! R-115 that #12 took over): `char`, `[u8; N]` under `#[cbor(bytes)]`, and with `heapless`, a
//! `String` and a byte `Vec`.

mod common;

use cbor_static::{Cbor, ErrorKind};
use common::{chunked_decode, chunked_decode_strict, decode, decode_strict, golden};

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Key {
    #[cbor(bytes)]
    id: [u8; 4],
    initial: char,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Digest(#[cbor(bytes)] [u8; 2]);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
enum Id {
    Digest(Digest),
    Index(u32),
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Record(u8, #[cbor(bytes)] [u8; 2]);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
enum Packet {
    Signed {
        #[cbor(bytes)]
        mac: [u8; 2],
    },
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
enum Frame {
    Raw(#[cbor(bytes)] [u8; 2]),
    Words([u8; 2]),
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[repr(C, u8)]
#[cbor(index)]
enum Tabled {
    Raw(#[cbor(bytes)] [u8; 2]),
    Words([u8; 2]),
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
enum Blob {
    Raw(#[cbor(bytes)] [u8; 2]),
    Words([u8; 2]),
}

#[test]
fn a_newtype_of_bytes_is_a_byte_string_an_untagged_enum_tells_apart() {
    golden(&Digest([0xab, 0xcd]), b"\x42\xab\xcd");
    golden(&Id::Digest(Digest([1, 2])), b"\x42\x01\x02");
    golden(&Id::Index(7), b"\x07");
}

#[test]
fn bytes_mark_a_tuple_struct_field_and_a_struct_variant_field() {
    golden(&Record(7, [1, 2]), b"\x82\x07\x42\x01\x02");
    golden(
        &Packet::Signed { mac: [1, 2] },
        b"\xa1\x66Signed\xa1\x63mac\x42\x01\x02",
    );
}

#[test]
fn bytes_mark_a_newtype_variant_field_through_glue_tables_and_untagged() {
    golden(&Frame::Raw([1, 2]), b"\xa1\x63Raw\x42\x01\x02");
    golden(&Frame::Words([1, 2]), b"\xa1\x65Words\x82\x01\x02");
    golden(&Tabled::Raw([1, 2]), b"\x82\x00\x42\x01\x02");
    golden(&Tabled::Words([1, 2]), b"\x82\x01\x82\x01\x02");
    golden(&Blob::Raw([1, 2]), b"\x42\x01\x02");
    golden(&Blob::Words([1, 2]), b"\x82\x01\x02");
    assert_eq!(
        decode::<Frame>(b"\xa1\x63Raw\x82\x01\x02"),
        Err(ErrorKind::TypeMismatch)
    );
}

#[test]
fn a_char_is_a_text_string_of_one_character_of_any_width() {
    for (one, bytes) in [
        ('a', b"\x61a".as_slice()),
        ('é', b"\x62\xc3\xa9"),
        ('€', b"\x63\xe2\x82\xac"),
        ('😀', b"\x64\xf0\x9f\x98\x80"),
    ] {
        golden(&one, bytes);
    }
    for not_one in [b"\x60".as_slice(), b"\x62ab", b"\x65abcde", b"\x7f\xff"] {
        assert_eq!(
            decode::<char>(not_one),
            Err(ErrorKind::TypeMismatch),
            "{not_one:x?}"
        );
    }
    assert_eq!(decode::<char>(b"\x41a"), Err(ErrorKind::TypeMismatch));
    assert_eq!(decode::<char>(b"\x62\xc3\x28"), Err(ErrorKind::InvalidUtf8));
}

#[test]
fn a_char_may_arrive_as_chunks_each_whole_characters() {
    for chunk in 1..=6 {
        assert_eq!(
            chunked_decode::<char>(b"\x7f\x62\xc3\xa9\xff", chunk),
            Ok('é')
        );
        assert_eq!(
            chunked_decode::<char>(b"\x7f\x61\xc3\x61\xa9\xff", chunk),
            Err(ErrorKind::InvalidUtf8)
        );
    }
}

#[test]
fn a_bytes_array_is_a_byte_string_of_exactly_its_length() {
    golden(
        &Key {
            id: [1, 2, 3, 4],
            initial: 'k',
        },
        b"\xa2\x62id\x44\x01\x02\x03\x04\x67initial\x61k",
    );
    for wrong in [
        b"\xa2\x62id\x43\x01\x02\x03\x67initial\x61k".as_slice(),
        b"\xa2\x62id\x45\x01\x02\x03\x04\x05\x67initial\x61k",
        b"\xa2\x62id\x64abcd\x67initial\x61k",
    ] {
        assert_eq!(
            decode::<Key>(wrong),
            Err(ErrorKind::TypeMismatch),
            "{wrong:x?}"
        );
    }
    assert_eq!(
        decode::<Key>(b"\xa2\x62id\x5f\x42\x01\x02\x40\x42\x03\x04\xff\x67initial\x61k"),
        Ok(Key {
            id: [1, 2, 3, 4],
            initial: 'k',
        })
    );
}

#[test]
fn chunks_must_be_definite_strings_of_the_same_type() {
    for bad in [
        b"\x5f\x41\x01\x7f\xff\xff".as_slice(),
        b"\x5f\x41\x01\x61a\x41\x02\x41\x03\xff",
        b"\x5f\x01\xff",
    ] {
        assert_eq!(
            decode::<Key>(&[b"\xa2\x62id".as_slice(), bad, b"\x67initial\x61k"].concat()),
            Err(ErrorKind::NotWellFormed),
            "{bad:x?}"
        );
    }
}

#[test]
fn strict_takes_owned_strings_only_definite() {
    assert_eq!(decode_strict::<char>(b"\x61a"), Ok('a'));
    assert_eq!(
        decode_strict::<char>(b"\x7f\x61a\xff"),
        Err(ErrorKind::NotDeterministic)
    );
    assert_eq!(
        decode_strict::<char>(b"\x78\x01a"),
        Err(ErrorKind::NotDeterministic)
    );
    for chunk in 1..=4 {
        assert_eq!(
            chunked_decode_strict::<char>(b"\x62\xc3\xa9", chunk),
            Ok('é')
        );
    }
}

#[cfg(feature = "heapless")]
mod bounded {
    use cbor_static::{Cbor, ErrorKind};
    use heapless::{String, Vec};

    use super::common::golden_in_place;

    #[derive(Cbor, Debug, PartialEq)]
    struct Note {
        title: String<12>,
        #[cbor(bytes)]
        body: Vec<u8, 4, u8>,
    }

    fn note(title: &str, body: &[u8]) -> Note {
        Note {
            title: String::try_from(title).expect("fits"),
            body: Vec::from_slice(body).expect("fits"),
        }
    }

    /// Whether `bytes` decode to `expected` in input chunks of every size.
    fn decodes_in_any_chunks(bytes: &[u8], expected: &Note) -> bool {
        (1..=bytes.len()).all(|chunk| {
            let mut slot = core::mem::MaybeUninit::uninit();
            let mut decoder = cbor_static::Decoder::<Note>::new(&mut slot);
            for piece in bytes.chunks(chunk) {
                if decoder.feed(piece).is_err() {
                    return false;
                }
            }
            decoder.finish().is_ok_and(|decoded| decoded == expected)
        })
    }

    #[test]
    fn a_string_and_a_byte_vec_fill_up_to_their_capacity() {
        golden_in_place(
            &note("é€😀", &[1, 2]),
            b"\xa2\x65title\x69\xc3\xa9\xe2\x82\xac\xf0\x9f\x98\x80\x64body\x42\x01\x02",
        );
        golden_in_place(&note("", &[]), b"\xa2\x65title\x60\x64body\x40");
        golden_in_place(&String::<1, u8>::try_from("a").expect("fits"), b"\x61a");
        for over in [
            b"\xa2\x65title\x6dabcdefghijklm\x64body\x40".as_slice(),
            b"\xa2\x65title\x60\x64body\x45\x01\x02\x03\x04\x05",
        ] {
            let mut slot = core::mem::MaybeUninit::<Note>::uninit();
            assert_eq!(
                cbor_static::decode_into(over, &mut slot)
                    .map(|_| ())
                    .map_err(|e| e.kind),
                Err(ErrorKind::OutOfRange),
                "{over:x?}"
            );
        }
    }

    #[test]
    fn indefinite_strings_fill_them_chunk_by_chunk() {
        assert!(decodes_in_any_chunks(
            b"\xa2\x65title\x7f\x62ab\x60\x63\xe2\x82\xac\xff\x64body\x5f\x41\x01\x42\x02\x03\xff",
            &note("ab€", &[1, 2, 3])
        ));
        let mut slot = core::mem::MaybeUninit::<Note>::uninit();
        assert_eq!(
            cbor_static::decode_into(
                b"\xa2\x65title\x7f\x61\xe2\x62\x82\xac\xff\x64body\x40",
                &mut slot
            )
            .map(|_| ())
            .map_err(|e| e.kind),
            Err(ErrorKind::InvalidUtf8)
        );
    }

    #[test]
    fn strict_rejects_an_indefinite_string_into_them() {
        let mut slot = core::mem::MaybeUninit::<String<4>>::uninit();
        assert_eq!(
            cbor_static::decode_strict_into(b"\x7f\x61a\xff", &mut slot)
                .map(|_| ())
                .map_err(|e| e.kind),
            Err(ErrorKind::NotDeterministic)
        );
    }
}
