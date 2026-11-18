#![forbid(unsafe_code)]
//! `from_slice` and `from_slice_partial` (R-203, R-205): by-value decoding of a whole slice, and of
//! the items of an RFC 8742 CBOR sequence one at a time.

use core::cell::Cell;

use cbor_static::{Cbor, Decode, ErrorKind, from_slice, from_slice_partial};

/// RFC 8742 §2's decoding of a CBOR sequence: none for no bytes, or one item then the sequence the
/// rest of the bytes make up.
fn sequence<'de, T: Decode<'de>>(input: &'de [u8]) -> Result<Vec<T>, ErrorKind> {
    match input {
        [] => Ok(Vec::new()),
        _ => from_slice_partial::<T>(input)
            .map_err(|e| e.kind)
            .and_then(|(first, rest)| {
                sequence(rest).map(|rest| core::iter::once(first).chain(rest).collect())
            }),
    }
}

#[test]
fn rfc_8742_sequences_of_three_integers_and_of_one_array() {
    assert_eq!(sequence::<u8>(b"\x01\x02\x03"), Ok(vec![1, 2, 3]));
    assert_eq!(
        sequence::<[u8; 3]>(b"\x83\x01\x02\x03"),
        Ok(vec![[1, 2, 3]])
    );
    assert_eq!(
        sequence::<u8>(b"\x83\x01\x02\x03"),
        Err(ErrorKind::TypeMismatch)
    );
}

#[test]
fn rfc_8742_the_empty_sequence_holds_no_items() {
    assert_eq!(sequence::<u8>(b""), Ok(vec![]));
    assert_eq!(
        from_slice_partial::<u8>(b"").map_err(|e| e.kind),
        Err(ErrorKind::UnexpectedEnd)
    );
}

#[test]
fn rfc_8742_a_truncated_last_item_is_detected() {
    assert_eq!(
        sequence::<u16>(b"\x01\x02\x19\x01"),
        Err(ErrorKind::UnexpectedEnd)
    );
    assert_eq!(
        sequence::<&str>(b"\x61a\x62b"),
        Err(ErrorKind::UnexpectedEnd)
    );
}

#[test]
fn rfc_8742_an_item_not_well_formed_stops_the_sequence() {
    assert_eq!(
        sequence::<u8>(b"\x01\x1c\x02"),
        Err(ErrorKind::NotWellFormed)
    );
}

#[test]
fn items_of_a_sequence_borrow_from_it() {
    let input = b"\x61a\x62bc\x01";
    let (first, rest) = from_slice_partial::<&str>(input).expect("first");
    let (second, rest) = from_slice_partial::<&str>(rest).expect("second");
    assert_eq!((first, second, rest), ("a", "bc", b"\x01".as_slice()));
}

#[derive(Cbor, Debug, PartialEq)]
struct Reading<'a> {
    id: u32,
    name: &'a str,
}

#[test]
fn from_slice_takes_exactly_one_item() {
    assert_eq!(
        from_slice::<Reading>(b"\xa2\x62id\x07\x64name\x61a"),
        Ok(Reading { id: 7, name: "a" })
    );
    assert_eq!(
        from_slice::<Reading>(b"\xa2\x62id\x07\x64name\x61a\x00").map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::TrailingInput, 12))
    );
    assert_eq!(
        from_slice::<Reading>(b"\xa2\x62id\x07\x64name\x62a").map_err(|e| e.kind),
        Err(ErrorKind::UnexpectedEnd)
    );
    assert_eq!(
        from_slice::<u8>(b"").map_err(|e| e.kind),
        Err(ErrorKind::UnexpectedEnd)
    );
}

std::thread_local! {
    static DROPPED: Cell<usize> = const { Cell::new(0) };
}

#[derive(Cbor, Debug, PartialEq, Eq)]
struct Counted(u32);

impl Drop for Counted {
    fn drop(&mut self) {
        DROPPED.set(DROPPED.get() + 1);
    }
}

#[test]
fn a_value_moved_out_is_dropped_once_by_its_owner() {
    DROPPED.set(0);
    let pair = from_slice::<(Counted, Counted)>(b"\x82\x01\x02").expect("decodes");
    assert_eq!(DROPPED.get(), 0);
    drop(pair);
    assert_eq!(DROPPED.get(), 2);

    DROPPED.set(0);
    let (first, rest) = from_slice_partial::<Counted>(b"\x01\x02").expect("decodes");
    assert_eq!((DROPPED.get(), rest), (0, b"\x02".as_slice()));
    drop(first);
    assert_eq!(DROPPED.get(), 1);
}

#[test]
fn a_failed_decode_drops_exactly_what_it_decoded() {
    DROPPED.set(0);
    assert!(from_slice::<(Counted, Counted)>(b"\x82\x01\xf6").is_err());
    assert_eq!(DROPPED.get(), 1);

    DROPPED.set(0);
    assert!(from_slice::<(Counted, Counted)>(b"\x82\x01\x02\x03").is_err());
    assert_eq!(DROPPED.get(), 2);

    DROPPED.set(0);
    assert!(from_slice_partial::<(Counted, Counted)>(b"\x82\x01").is_err());
    assert_eq!(DROPPED.get(), 1);
}
