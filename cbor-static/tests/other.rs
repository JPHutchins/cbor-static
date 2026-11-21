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

/// A `skip`ped field and an `other` field have reserved names no key matches, so neither clashes
/// with the other, nor two `skip`ped fields with each other.
#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Reserved {
    id: u8,
    #[serde(skip)]
    cache: u8,
    #[cbor(other)]
    unknown: Skipped,
    #[serde(skip)]
    spare: u8,
}

#[test]
fn skipped_and_other_fields_share_no_key() {
    let reserved = decode::<Reserved>(UNKNOWN).expect("decodes");
    assert_eq!(
        (
            reserved.id,
            reserved.cache,
            reserved.spare,
            reserved.unknown.count()
        ),
        (1, 0, 0, 4)
    );
    assert_eq!(encode(&reserved), b"\xa1\x62id\x01");
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

/// #79: an `other` capture refuses a key it already holds, compared as a field's key is.
#[cfg(feature = "heapless")]
mod repeated {
    use core::mem::MaybeUninit;

    use cbor_static::{Cbor, Decoder, ErrorKind, Fed, RawEntry, decode_into, decode_strict_into};
    use heapless::Vec;

    /// An integer-keyed map, whose text keys are unknown entries too.
    #[derive(Cbor, Debug)]
    struct Labels<'a> {
        #[cbor(key = 1)]
        alg: Option<i8>,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>, 4>,
    }

    /// A text-keyed map, whose text keys are matched before they are captured.
    #[derive(Cbor, Debug)]
    struct Named<'a> {
        id: Option<u8>,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>, 4>,
    }

    const REST: u16 = 1;

    fn labels(bytes: &[u8]) -> Result<usize, (ErrorKind, u16, u32)> {
        let mut slot = MaybeUninit::uninit();
        decode_into::<Labels>(bytes, &mut slot)
            .map(|labels| labels.rest.len())
            .map_err(|e| (e.kind, e.field, e.offset))
    }

    fn named(bytes: &[u8]) -> Result<usize, (ErrorKind, u16, u32)> {
        let mut slot = MaybeUninit::uninit();
        decode_into::<Named>(bytes, &mut slot)
            .map(|named| named.rest.len())
            .map_err(|e| (e.kind, e.field, e.offset))
    }

    /// `bytes` fed one piece per `cut`, each entry whole in its piece.
    fn labels_in_pieces(bytes: &[u8], cuts: &[usize]) -> Result<usize, (ErrorKind, u16, u32)> {
        let mut slot = MaybeUninit::<Labels>::uninit();
        let mut decoder = Decoder::new(&mut slot);
        [0].iter()
            .chain(cuts)
            .zip(cuts.iter().chain([&bytes.len()]))
            .try_for_each(|(&start, &end)| decoder.feed(&bytes[start..end]).map(|_: Fed| ()))
            .and_then(|()| decoder.finish().map(|labels| labels.rest.len()))
            .map_err(|e| (e.kind, e.field, e.offset))
    }

    macro_rules! cases {
        ($decode:ident: $($name:ident: $bytes:expr => $expected:expr;)*) => {$(
            #[test]
            fn $name() {
                assert_eq!($decode($bytes), $expected);
            }
        )*};
    }

    mod refused {
        use super::{ErrorKind, REST, labels, named};

        cases! { labels:
            a_negative_label: b"\xa2\x20\x01\x20\x02" => Err((ErrorKind::DuplicateKey, REST, 4));
            a_label_in_a_wider_head: b"\xa2\x20\x01\x38\x00\x02" => Err((ErrorKind::DuplicateKey, REST, 5));
            a_text_key: b"\xa2\x61z\x01\x61z\x02" => Err((ErrorKind::DuplicateKey, REST, 6));
            a_text_key_of_a_wider_length: b"\xa2\x61z\x01\x78\x01z\x02" => Err((ErrorKind::DuplicateKey, REST, 7));
            an_array_key: b"\xa2\x81\x01\x01\x81\x01\x02" => Err((ErrorKind::DuplicateKey, REST, 6));
            an_indefinite_text_key: b"\xa2\x7f\x61z\xff\x01\x7f\x61z\xff\x02" => Err((ErrorKind::DuplicateKey, REST, 10));
            a_label_past_a_named_one: b"\xa3\x20\x01\x01\x26\x20\x02" => Err((ErrorKind::DuplicateKey, REST, 6));
            a_label_once_the_capture_is_full: b"\xa5\x20\x00\x21\x00\x22\x00\x23\x00\x20\x00" => Err((ErrorKind::DuplicateKey, REST, 10));
        }

        cases! { named:
            a_matched_text_key: b"\xa2\x61z\x01\x61z\x02" => Err((ErrorKind::DuplicateKey, REST, 6));
            a_matched_text_key_past_a_named_one: b"\xa3\x61z\x01\x62id\x07\x61z\x02" => Err((ErrorKind::DuplicateKey, REST, 10));
        }
    }

    mod refused_in_pieces {
        use super::{ErrorKind, REST, labels_in_pieces};

        #[test]
        fn a_text_key_one_piece_per_entry() {
            assert_eq!(
                labels_in_pieces(b"\xa2\x61z\x01\x61z\x02", &[4]),
                Err((ErrorKind::DuplicateKey, REST, 6))
            );
        }

        /// A capture takes an entry only from a chunk that holds it whole.
        #[test]
        fn a_cut_inside_a_text_key_needs_contiguous_input() {
            assert_eq!(
                labels_in_pieces(b"\xa2\x61z\x01\x61z\x02", &[2]),
                Err((ErrorKind::NeedContiguous, REST, 2))
            );
        }

        #[test]
        fn a_negative_label_one_piece_per_entry() {
            assert_eq!(
                labels_in_pieces(b"\xa2\x20\x01\x20\x02", &[3]),
                Err((ErrorKind::DuplicateKey, REST, 4))
            );
        }
    }

    /// Distinct keys, and the encodings of one key this comparison does not equate: a non-preferred
    /// head nested in a key, one float in two widths, and definite against indefinite text.
    mod captured {
        use super::labels;

        cases! { labels:
            distinct_labels: b"\xa2\x20\x01\x21\x02" => Ok(2);
            an_unsigned_and_a_negative_label_of_one_argument: b"\xa2\x00\x01\x20\x02" => Ok(2);
            text_and_bytes_of_one_content: b"\xa2\x61z\x01\x41z\x02" => Ok(2);
            false_and_the_half_float_of_its_bits: b"\xa2\xf4\x01\xf9\x00\x14\x02" => Ok(2);
            a_non_preferred_head_nested_in_a_key: b"\xa2\x81\x01\x01\x81\x18\x01\x02" => Ok(2);
            one_float_in_two_widths: b"\xa2\xf9\x3c\x00\x01\xfa\x3f\x80\x00\x00\x02" => Ok(2);
            definite_and_indefinite_text: b"\xa2\x61z\x01\x7f\x61z\xff\x02" => Ok(2);
        }
    }

    /// A text key that overflows the capture is reported past the whole key.
    #[test]
    fn an_overflowing_text_key_is_reported_past_it() {
        assert_eq!(
            named(b"\xa5\x61a\x00\x61b\x00\x61c\x00\x61d\x00\x61e\x00"),
            Err((ErrorKind::LimitExceeded, REST, 15))
        );
    }

    /// Strict refuses the first unknown entry before any can repeat.
    mod strict {
        use super::{Decoder, ErrorKind, Labels, MaybeUninit, decode_strict_into};

        /// Its key's head is all of the entry this chunk holds.
        #[test]
        fn a_text_key_whose_value_is_still_to_come() {
            let mut slot = MaybeUninit::<Labels>::uninit();
            assert_eq!(
                Decoder::new_strict(&mut slot)
                    .feed(b"\xa2\x61z")
                    .map(|_| ())
                    .map_err(|e| (e.kind, e.offset)),
                Err((ErrorKind::UnknownField, 2))
            );
        }

        fn strict(bytes: &[u8]) -> Result<usize, (ErrorKind, u32)> {
            let mut slot = MaybeUninit::uninit();
            decode_strict_into::<Labels>(bytes, &mut slot)
                .map(|labels| labels.rest.len())
                .map_err(|e| (e.kind, e.offset))
        }

        cases! { strict:
            a_negative_label: b"\xa2\x20\x01\x20\x02" => Err((ErrorKind::UnknownField, 2));
            a_label_in_a_wider_head: b"\xa2\x20\x01\x38\x00\x02" => Err((ErrorKind::UnknownField, 2));
            a_text_key: b"\xa2\x61z\x01\x61z\x02" => Err((ErrorKind::UnknownField, 2));
        }
    }
}

#[cfg(feature = "alloc")]
mod allocated {
    use core::mem::MaybeUninit;

    use cbor_static::{Cbor, Decode, Decoder, ErrorKind, RawEntry, from_slice};

    use super::UNKNOWN;
    use super::common::{chunked_encode, encode};

    #[derive(Cbor, Debug, PartialEq)]
    struct Header<'a> {
        id: u8,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>>,
        seq: u8,
    }

    #[derive(Cbor, Debug, PartialEq)]
    struct Extra<'a> {
        tag: u8,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>>,
    }

    #[derive(Cbor, Debug, PartialEq)]
    struct Parent<'a> {
        id: u8,
        #[serde(flatten)]
        extra: Extra<'a>,
    }

    #[test]
    fn every_unknown_entry_is_captured_and_written_in_its_fields_place() {
        let header: Header = from_slice(UNKNOWN).expect("decodes");
        assert_eq!(
            header
                .rest
                .iter()
                .map(|entry| (entry.key.as_bytes(), entry.value.as_bytes()))
                .collect::<Vec<_>>(),
            [
                (b"\x05".as_slice(), b"\x06".as_slice()),
                (b"\x81\x01", b"\x61v"),
                (b"\x61x", b"\xf5"),
            ]
        );
        let written = b"\xa5\x62id\x01\x05\x06\x81\x01\x61v\x61x\xf5\x63seq\x02";
        for chunk in 1..=written.len() {
            assert_eq!(chunked_encode(&header, chunk), written, "chunks of {chunk}");
        }
        assert_eq!(
            from_slice::<Header>(written).as_ref().map(encode),
            Ok(written.to_vec())
        );
        let many: Header = from_slice(b"\xa6\x62id\x01\x63seq\x02\x00\x00\x01\x00\x02\x00\x03\x00")
            .expect("decodes");
        assert_eq!(many.rest.len(), 4);
    }

    #[test]
    fn a_captured_entry_split_across_chunks_needs_contiguous_input() {
        const { assert!(<Header<'_> as Decode<'_>>::CONTIGUOUS) };
        let mut slot = MaybeUninit::<Header>::uninit();
        let mut decoder = Decoder::new(&mut slot);
        assert_eq!(
            decoder.feed(&UNKNOWN[..6]).map_err(|e| e.kind),
            Err(ErrorKind::NeedContiguous)
        );
    }

    #[derive(Cbor, Debug)]
    struct Labels<'a> {
        #[cbor(key = 1)]
        alg: Option<i8>,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>>,
    }

    const REST: u16 = 1;

    fn labels(bytes: &[u8]) -> Result<usize, (ErrorKind, u16, u32)> {
        from_slice::<Labels>(bytes)
            .map(|labels| labels.rest.len())
            .map_err(|e| (e.kind, e.field, e.offset))
    }

    /// `count` distinct unsigned labels from 2 in three-byte heads, then `last`, in a map of them all.
    fn many_labels(count: u16, last: &[u8]) -> Vec<u8> {
        [
            [0xb9].as_slice(),
            &(count + 1).to_be_bytes(),
            &(2..count + 2)
                .flat_map(|label| [[0x19].as_slice(), &label.to_be_bytes(), &[0xf6]].concat())
                .collect::<Vec<u8>>(),
            last,
            &[0xf6],
        ]
        .concat()
    }

    const MANY: u16 = if cfg!(miri) { 64 } else { 2048 };

    macro_rules! cases {
        ($($name:ident: $bytes:expr => $expected:expr;)*) => {$(
            #[test]
            fn $name() {
                assert_eq!(labels($bytes), $expected);
            }
        )*};
    }

    /// #79: each key is refused when the capture already holds it, compared as a field's key is,
    /// and what the capture took is dropped.
    mod refused {
        use super::{ErrorKind, REST, labels};

        cases! {
            a_negative_label: b"\xa2\x20\x01\x20\x02" => Err((ErrorKind::DuplicateKey, REST, 4));
            a_label_in_a_wider_head: b"\xa2\x20\x01\x38\x00\x02" => Err((ErrorKind::DuplicateKey, REST, 5));
            a_text_key: b"\xa2\x61z\x01\x61z\x02" => Err((ErrorKind::DuplicateKey, REST, 6));
            an_array_key_after_others: b"\xa4\x81\x01\x01\x61z\x02\x20\x03\x81\x01\x04" => Err((ErrorKind::DuplicateKey, REST, 11));
        }
    }

    /// The capture is unbounded, and checks each key against every key it holds before it.
    #[test]
    fn an_unbounded_capture_checks_each_key_against_every_earlier_one() {
        assert_eq!(
            labels(&many_labels(MANY, b"\x20")),
            Ok(usize::from(MANY) + 1)
        );
        let repeated = many_labels(MANY, b"\x02");
        assert_eq!(
            labels(&repeated),
            Err((
                ErrorKind::DuplicateKey,
                REST,
                u32::try_from(repeated.len() - 1).expect("fits")
            ))
        );
    }

    #[test]
    fn a_flattened_structs_other_field_takes_its_parents_unknown_entries() {
        let parent: Parent =
            from_slice(b"\xa4\x62id\x01\x61z\x03\x63tag\x02\x61y\x04").expect("decodes");
        assert_eq!(
            (parent.id, parent.extra.tag, parent.extra.rest.len()),
            (1, 2, 2)
        );
        assert_eq!(
            encode(&parent),
            b"\xa4\x62id\x01\x63tag\x02\x61z\x03\x61y\x04"
        );
    }
}
