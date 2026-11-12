//! Raw items (#12 Q23, Q24, D-11a): `RawCbor`, `ArrayRef` and `MapRef` borrow a whole item from
//! contiguous input, are checked when decoded so iterating cannot fail, and encode verbatim.

mod common;

use core::mem::MaybeUninit;

use cbor_static::{ArrayRef, Cbor, Decoder, ErrorKind, Fed, MapRef, RawCbor};
use common::encode;

#[derive(Cbor, Debug, PartialEq)]
struct Point {
    x: u32,
    y: u32,
}

#[derive(Cbor)]
struct Envelope<'a> {
    kind: u32,
    body: RawCbor<'a>,
}

fn with<'de, T: cbor_static::Decode<'de>, R>(
    bytes: &'de [u8],
    then: impl FnOnce(&T) -> R,
) -> Result<R, ErrorKind> {
    let mut slot = MaybeUninit::<T>::uninit();
    cbor_static::decode_into(bytes, &mut slot)
        .map(|value| then(value))
        .map_err(|e| e.kind)
}

#[test]
fn a_raw_item_is_any_whole_item_kept_and_written_verbatim() {
    for item in [
        b"\x18\x01".as_slice(),
        b"\x9f\x01\x82\x02\x03\xff",
        b"\xbf\x61a\x7f\x61b\xff\xff",
        b"\xc1\x1a\x51\x4b\x67\xb0",
        b"\xf6",
    ] {
        assert_eq!(
            with::<RawCbor, _>(item, |raw| raw.as_bytes().to_vec()),
            Ok(item.to_vec())
        );
        let mut input = b"\xa2\x64kind\x01\x64body".to_vec();
        input.extend_from_slice(item);
        assert_eq!(
            with::<Envelope, _>(&input, encode),
            Ok(input.clone()),
            "{item:x?}"
        );
    }
}

#[test]
fn a_raw_item_must_be_well_formed_and_whole_in_one_chunk() {
    for (bytes, error) in [
        (b"\x9f\x01\x82\x02\xff".as_slice(), ErrorKind::NotWellFormed),
        (b"\x62\xc3\x28", ErrorKind::InvalidUtf8),
        (b"\x82\x01", ErrorKind::UnexpectedEnd),
        (b"\xbf\x01\xff", ErrorKind::NotWellFormed),
    ] {
        assert_eq!(with::<RawCbor, _>(bytes, |_| ()), Err(error), "{bytes:x?}");
    }
    for split in [1, 2, 3] {
        let mut slot = MaybeUninit::<RawCbor>::uninit();
        let mut decoder = Decoder::new(&mut slot);
        let bytes = b"\x19\x01\x00";
        assert_eq!(
            decoder
                .feed(&bytes[..split])
                .and_then(|_| decoder.feed(&bytes[split..]))
                .map_err(|e| e.kind),
            if split == bytes.len() {
                Ok(Fed::Done(0))
            } else {
                Err(ErrorKind::NeedContiguous)
            },
            "split at {split}"
        );
    }
}

#[test]
fn an_array_ref_yields_every_element_it_checked() {
    assert_eq!(
        with::<ArrayRef<u16>, _>(b"\x83\x01\x19\x01\x00\x18\x18", |values| {
            (
                values.len(),
                values.is_empty(),
                values.iter().collect::<Vec<_>>(),
            )
        }),
        Ok((3, false, vec![1, 256, 24]))
    );
    assert_eq!(
        with::<ArrayRef<u16>, _>(b"\x9f\x01\x02\xff", |values| values
            .iter()
            .collect::<Vec<_>>()),
        Ok(vec![1, 2])
    );
    assert_eq!(
        with::<ArrayRef<u16>, _>(b"\x80", |values| (values.is_empty(), values.iter().count())),
        Ok((true, 0))
    );
    assert_eq!(
        with::<ArrayRef<Point>, _>(b"\x82\xa2\x61x\x01\x61y\x02\xa1\x61x\x03", |_| ()),
        Err(ErrorKind::MissingField)
    );
    assert_eq!(
        with::<ArrayRef<Point>, _>(b"\x81\xa2\x61y\x02\x61x\x01", |points| {
            points.iter().collect::<Vec<_>>()
        }),
        Ok(vec![Point { x: 1, y: 2 }])
    );
}

#[test]
fn a_malformed_or_mistyped_element_fails_the_whole_decode() {
    for (bytes, error) in [
        (b"\x82\x01\x61a".as_slice(), ErrorKind::TypeMismatch),
        (b"\x82\x01\x1a\x00\x01\x00\x00", ErrorKind::OutOfRange),
        (b"\x82\x01\x62\xc3\x28", ErrorKind::InvalidUtf8),
        (b"\xa1\x01\x02", ErrorKind::TypeMismatch),
    ] {
        assert_eq!(
            with::<ArrayRef<u16>, _>(bytes, |_| ()),
            Err(error),
            "{bytes:x?}"
        );
    }
}

#[test]
fn a_map_ref_yields_every_entry_it_checked() {
    assert_eq!(
        with::<MapRef<&str, u8>, _>(b"\xa2\x61a\x01\x61b\x02", |map| {
            (map.len(), map.iter().collect::<Vec<_>>())
        }),
        Ok((2, vec![("a", 1), ("b", 2)]))
    );
    assert_eq!(
        with::<MapRef<&str, u8>, _>(b"\xbf\x61a\x01\xff", |map| map.iter().collect::<Vec<_>>()),
        Ok(vec![("a", 1)])
    );
    assert_eq!(
        with::<MapRef<&str, u8>, _>(b"\xa1\x61a\x19\x01\x00", |_| ()),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        with::<MapRef<&str, u8>, _>(b"\x81\x01", |_| ()),
        Err(ErrorKind::TypeMismatch)
    );
}

/// A view keeps the map as it arrived: a repeated key is the caller's to see, unless `Strict`, whose
/// rising key order excludes it.
#[test]
fn a_map_ref_yields_a_repeated_key_as_it_arrived_unless_strict() {
    let bytes = b"\xa2\x01\x02\x01\x03";
    assert_eq!(
        with::<MapRef<u8, u8>, _>(bytes, |map| map.iter().collect::<Vec<_>>()),
        Ok(vec![(1, 2), (1, 3)])
    );
    let mut slot = MaybeUninit::<MapRef<u8, u8>>::uninit();
    assert_eq!(
        cbor_static::decode_strict_into(bytes, &mut slot)
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ErrorKind::NotDeterministic)
    );
}

#[test]
fn views_nest_and_encode_as_they_arrived() {
    let input = b"\x82\x9f\x18\x01\xff\x81\x02";
    assert_eq!(
        with::<ArrayRef<ArrayRef<u8>>, _>(input, |outer| {
            (
                outer
                    .iter()
                    .map(|inner| inner.iter().collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                encode(outer),
            )
        }),
        Ok((vec![vec![1], vec![2]], input.to_vec()))
    );
}

#[test]
fn strict_checks_every_element_strictly() {
    let mut slot = MaybeUninit::<ArrayRef<Point>>::uninit();
    assert_eq!(
        cbor_static::decode_strict_into(b"\x81\xa2\x61y\x02\x61x\x01", &mut slot)
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ErrorKind::NotDeterministic)
    );
    assert_eq!(
        cbor_static::decode_strict_into(b"\x81\xa3\x61x\x01\x61y\x02\x61z\x00", &mut slot)
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ErrorKind::UnknownField)
    );
    assert_eq!(
        cbor_static::decode_strict_into(b"\x81\xa2\x61x\x01\x61y\x02", &mut slot)
            .map(|points| points.iter().collect::<Vec<_>>())
            .map_err(|e| e.kind),
        Ok(vec![Point { x: 1, y: 2 }])
    );
}
