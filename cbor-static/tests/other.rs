#![forbid(unsafe_code)]
//! `#[cbor(other)]` (R-128, #14 Q35): a map's entries that no field claims go to the struct's one
//! `other` field, which counts them (`Skipped`) or captures each whole (`Vec<RawEntry>`), and writes
//! the captured ones back in its place, verbatim.

mod common;

use cbor_static::{Cbor, ErrorKind, Skipped};
use common::{chunked_decode, decode, decode_strict, encode};

/// `id`, an integer key, `seq`, an array key and a text key: three entries no field claims.
const UNKNOWN: &[u8] = b"\xa5\x62id\x01\x05\x06\x63seq\x02\x81\x01\x61v\x61x\xf5";

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Counting {
    id: u8,
    #[cbor(other)]
    unknown: Skipped,
    seq: u8,
}

#[test]
fn skipped_counts_unknown_entries_in_any_chunks_and_is_never_written() {
    for chunk in 1..=UNKNOWN.len() {
        let counted = chunked_decode::<Counting>(UNKNOWN, chunk).expect("decodes");
        assert_eq!(
            (counted.id, counted.seq, counted.unknown.count()),
            (1, 2, 3),
            "chunks of {chunk}"
        );
    }
    assert_eq!(
        decode::<Counting>(b"\xa2\x62id\x01\x63seq\x02").map(|c| c.unknown.count()),
        Ok(0)
    );
    assert_eq!(
        encode(&decode::<Counting>(UNKNOWN).expect("decodes")),
        b"\xa2\x62id\x01\x63seq\x02"
    );
}

/// A counted entry's value is checked alike whether one chunk holds the entry, which is then walked
/// once, or chunks split it, which the chunked skip then walks.
#[test]
fn a_counted_entrys_malformed_value_fails_alike_in_any_chunks() {
    let cases: [(&[u8], ErrorKind); 6] = [
        (b"\x62\xff\xfe", ErrorKind::InvalidUtf8),
        (b"\x7f\x62\xff\xfe\xff", ErrorKind::InvalidUtf8),
        (b"\x7f\x41a\xff", ErrorKind::NotWellFormed),
        (b"\x7f\x61a\x7f\x61b\xff\xff", ErrorKind::NotWellFormed),
        (b"\x1c", ErrorKind::NotWellFormed),
        (b"\xf8\x10", ErrorKind::NotWellFormed),
    ];
    for (value, error) in cases {
        let input = [b"\xa3\x62id\x01\x61z".as_slice(), value, b"\x63seq\x02"].concat();
        for chunk in 1..=input.len() {
            assert_eq!(
                chunked_decode::<Counting>(&input, chunk),
                Err(error),
                "{value:02x?} in chunks of {chunk}"
            );
        }
    }
}

#[test]
fn strict_rejects_an_unknown_entry_though_other_would_take_it() {
    assert_eq!(
        decode_strict::<Counting>(b"\xa3\x61z\x03\x62id\x01\x63seq\x02"),
        Err(ErrorKind::UnknownField)
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct SortedCounting {
    seq: u8,
    #[cbor(other)]
    unknown: Skipped,
    id: u8,
}

#[test]
fn a_deterministic_struct_may_count_what_it_never_writes() {
    let counted = decode::<SortedCounting>(UNKNOWN).expect("decodes");
    assert_eq!(counted.unknown.count(), 3);
    assert_eq!(encode(&counted), b"\xa2\x62id\x01\x63seq\x02");
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Labelled {
    #[cbor(key = 1)]
    alg: i8,
    #[cbor(other)]
    unknown: Skipped,
}

#[test]
fn integer_keyed_and_mixed_maps_take_unknown_entries_too() {
    assert_eq!(
        decode::<Labelled>(b"\xa3\x01\x26\x61a\x00\x02\x00").map(|l| (l.alg, l.unknown.count())),
        Ok((-7, 2))
    );
}

#[cfg(feature = "heapless")]
mod captured {
    use core::mem::MaybeUninit;

    use cbor_static::{Cbor, Decoder, ErrorKind, RawEntry, decode_into};
    use heapless::Vec;

    use super::UNKNOWN;
    use super::common::{chunked_encode, encode};

    #[derive(Cbor, Debug, PartialEq)]
    struct Header<'a> {
        id: u8,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>, 3>,
        seq: u8,
    }

    fn entries<'a>(header: &Header<'a>) -> std::vec::Vec<(&'a [u8], &'a [u8])> {
        header
            .rest
            .iter()
            .map(|entry| (entry.key.as_bytes(), entry.value.as_bytes()))
            .collect()
    }

    #[test]
    fn captured_entries_keep_their_bytes_and_are_written_in_their_fields_place() {
        let mut slot = MaybeUninit::uninit();
        let header: &mut Header = decode_into(UNKNOWN, &mut slot).expect("decodes");
        assert_eq!((header.id, header.seq), (1, 2));
        assert_eq!(
            entries(header),
            [
                (b"\x05".as_slice(), b"\x06".as_slice()),
                (b"\x81\x01", b"\x61v"),
                (b"\x61x", b"\xf5"),
            ]
        );
        let written = b"\xa5\x62id\x01\x05\x06\x81\x01\x61v\x61x\xf5\x63seq\x02";
        for chunk in 1..=written.len() {
            assert_eq!(
                chunked_encode(&*header, chunk),
                written,
                "chunks of {chunk}"
            );
        }
        let mut slot = MaybeUninit::uninit();
        let again: &mut Header = decode_into(written, &mut slot).expect("decodes");
        assert_eq!(encode(&*again), written);
    }

    #[test]
    fn a_captured_entry_split_across_chunks_needs_contiguous_input() {
        let mut slot = MaybeUninit::<Header>::uninit();
        let mut decoder = Decoder::new(&mut slot);
        assert_eq!(
            decoder.feed(&UNKNOWN[..6]).map_err(|e| e.kind),
            Err(ErrorKind::NeedContiguous)
        );
    }

    #[test]
    fn one_entry_more_than_the_vec_holds_exceeds_its_limit() {
        let mut slot = MaybeUninit::<Header>::uninit();
        assert_eq!(
            decode_into(
                b"\xa6\x62id\x01\x63seq\x02\x00\x00\x01\x00\x02\x00\x03\x00",
                &mut slot
            )
            .map(|_| ())
            .map_err(|e| e.kind),
            Err(ErrorKind::LimitExceeded)
        );
    }

    #[derive(Cbor, Debug, PartialEq)]
    struct Body<'a> {
        a: u8,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>, 2>,
    }

    #[derive(Cbor, Debug, PartialEq)]
    #[serde(tag = "t", content = "c")]
    enum Adjacent<'a> {
        V(Body<'a>),
    }

    #[derive(Cbor, Debug, PartialEq)]
    enum External<'a> {
        V(Body<'a>),
    }

    #[derive(Cbor, Debug, PartialEq)]
    #[serde(untagged)]
    enum Untagged<'a> {
        Flag(bool),
        V(Body<'a>),
    }

    /// A payload whose own map holds its entries captures only its own unknown entry, and writes a
    /// head that counts it, whichever enum carries it; only an internally tagged enum, whose tag
    /// joins the payload's map, takes no `other` payload.
    #[test]
    fn a_payload_with_its_own_map_captures_only_its_own_unknown_entries() {
        let payload = b"\xa2\x61a\x01\x61z\x02";
        let cases: [std::vec::Vec<u8>; 3] = [
            [b"\xa2\x61t\x61V\x61c".as_slice(), payload].concat(),
            [b"\xa1\x61V".as_slice(), payload].concat(),
            payload.to_vec(),
        ];
        let mut slot = MaybeUninit::uninit();
        let adjacent: &mut Adjacent = decode_into(&cases[0], &mut slot).expect("decodes");
        let Adjacent::V(body) = &*adjacent;
        assert_eq!((body.a, body.rest.len()), (1, 1));
        assert_eq!(encode(&*adjacent), cases[0]);
        let mut slot = MaybeUninit::uninit();
        let external: &mut External = decode_into(&cases[1], &mut slot).expect("decodes");
        let External::V(body) = &*external;
        assert_eq!((body.a, body.rest.len()), (1, 1));
        assert_eq!(encode(&*external), cases[1]);
        let mut slot = MaybeUninit::uninit();
        let untagged: &mut Untagged = decode_into(&cases[2], &mut slot).expect("decodes");
        let Untagged::V(body) = &*untagged else {
            panic!("the payload's variant");
        };
        assert_eq!((body.a, body.rest.len()), (1, 1));
        assert_eq!(encode(&*untagged), cases[2]);
    }

    #[derive(Cbor, Debug, PartialEq)]
    struct Extra<'a> {
        tag: u8,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>, 2>,
    }

    #[derive(Cbor, Debug, PartialEq)]
    struct Parent<'a> {
        id: u8,
        #[serde(flatten)]
        extra: Extra<'a>,
    }

    #[test]
    fn a_flattened_structs_other_field_takes_its_parents_unknown_entries() {
        let mut slot = MaybeUninit::uninit();
        let parent: &mut Parent =
            decode_into(b"\xa3\x62id\x01\x61z\x03\x63tag\x02", &mut slot).expect("decodes");
        assert_eq!((parent.id, parent.extra.tag), (1, 2));
        assert_eq!(parent.extra.rest[0].key.as_bytes(), b"\x61z");
        assert_eq!(encode(&*parent), b"\xa3\x62id\x01\x63tag\x02\x61z\x03");
    }
}
