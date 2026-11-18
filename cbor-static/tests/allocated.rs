//! Alloc `Vec`, `String`, `#[cbor(bytes)]` `Vec<u8>` and `Box` (#5 Q8): round trips at every chunk
//! size, indefinite strings (D-13), `max_len` (#5 Q15), and the field an error inside one reports.
#![cfg(feature = "alloc")]

mod common;

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::{Cbor, Decode, Encode, ErrorKind, from_slice};
use common::{encode, golden_released};
use proptest::prelude::*;

/// `golden_in_place`, dropping each value decoded, which Miri checks frees all it allocated.
#[allow(unsafe_code)]
fn golden<T>(value: &T, bytes: &[u8])
where
    T: Encode + for<'de> Decode<'de> + PartialEq + Debug,
{
    golden_released(value, bytes, |slot: &mut MaybeUninit<T>| unsafe {
        slot.assume_init_drop();
    });
}

#[derive(Cbor, Debug, PartialEq)]
struct Record {
    id: u8,
    name: String,
    tags: Vec<String>,
    #[cbor(bytes)]
    blob: Vec<u8>,
    note: Option<String>,
}

#[derive(Cbor, Debug, PartialEq)]
struct Limited {
    #[cbor(max_len = 2)]
    name: String,
    #[cbor(max_len = 2)]
    values: Vec<u8>,
    #[cbor(bytes, max_len = 2)]
    blob: Vec<u8>,
}

#[test]
fn containers_round_trip_at_every_chunk_size() {
    golden(&vec![1u32, 2, 1000], b"\x83\x01\x02\x19\x03\xe8");
    golden(&Vec::<u32>::new(), b"\x80");
    golden(&vec![vec![1u16], vec![]], b"\x82\x81\x01\x80");
    golden(&String::from("hello"), b"\x65hello");
    golden(&String::new(), b"\x60");
    golden(
        &Record {
            id: 7,
            name: "ab".into(),
            tags: vec!["x".into(), String::new()],
            blob: vec![1, 2],
            note: None,
        },
        b"\xa5\x62id\x07\x64name\x62ab\x64tags\x82\x61x\x60\x64blob\x42\x01\x02\x64note\xf6",
    );
}

#[test]
fn indefinite_strings_and_arrays_decode() {
    assert_eq!(
        from_slice::<String>(b"\x7f\x62ab\x60\x61c\xff"),
        Ok("abc".into())
    );
    assert_eq!(
        from_slice::<Vec<String>>(b"\x9f\x7f\x61a\xff\x61b\xff"),
        Ok(vec!["a".into(), "b".into()])
    );
    assert_eq!(
        from_slice::<Record>(
            b"\xa5\x62id\x07\x64name\x60\x64tags\x80\x64blob\x5f\x41\x01\x40\x41\x02\xff\x64note\x61z"
        )
        .map(|record| (record.blob, record.note)),
        Ok((vec![1, 2], Some("z".into())))
    );
    assert_eq!(
        from_slice::<String>(b"\x7f\x41a\xff").map_err(|e| e.kind),
        Err(ErrorKind::NotWellFormed)
    );
}

#[test]
fn max_len_bounds_text_elements_and_bytes() {
    let within = Limited {
        name: "ab".into(),
        values: vec![1, 2],
        blob: vec![3, 4],
    };
    golden(
        &within,
        b"\xa3\x64name\x62ab\x66values\x82\x01\x02\x64blob\x42\x03\x04",
    );
    for longer in [
        b"\xa3\x64name\x63abc\x66values\x80\x64blob\x40".as_slice(),
        b"\xa3\x64name\x60\x66values\x83\x01\x02\x03\x64blob\x40",
        b"\xa3\x64name\x60\x66values\x9f\x01\x02\x03\xff\x64blob\x40",
        b"\xa3\x64name\x60\x66values\x80\x64blob\x43\x01\x02\x03",
        b"\xa3\x64name\x7f\x62ab\x61c\xff\x66values\x80\x64blob\x40",
    ] {
        assert_eq!(
            from_slice::<Limited>(longer).map_err(|e| e.kind),
            Err(ErrorKind::LimitExceeded),
            "{longer:x?}"
        );
    }
    let mut out = [0u8; 32];
    assert_eq!(
        cbor_static::to_slice(
            &Limited {
                name: "abc".into(),
                ..within
            },
            &mut out
        )
        .map_err(|e| e.kind),
        Err(ErrorKind::LimitExceeded)
    );
}

#[test]
fn an_error_inside_a_string_reports_its_field() {
    #[derive(Cbor, Debug)]
    struct Owned {
        id: u8,
        name: String,
    }

    #[derive(Cbor, Debug)]
    struct Borrowed<'a> {
        id: u8,
        name: &'a str,
    }

    let input = b"\xa2\x62id\x07\x64name\x62a\xff";
    let owned = from_slice::<Owned>(input)
        .map(|_| ())
        .map_err(|e| (e.kind, e.field));
    assert_eq!(owned, Err((ErrorKind::InvalidUtf8, 1)));
    assert_eq!(
        owned,
        from_slice::<Borrowed>(input)
            .map(|_| ())
            .map_err(|e| (e.kind, e.field))
    );
}

#[derive(Cbor, Debug, PartialEq)]
struct Boxed {
    id: u8,
    point: Box<(u16, String)>,
    unit: Box<()>,
    nested: Option<Box<[Box<u8>; 2]>>,
}

#[test]
fn boxes_round_trip_as_what_they_hold() {
    golden(&Box::new(500u32), b"\x19\x01\xf4");
    golden(&Box::new(String::from("ab")), b"\x62ab");
    golden(&Box::new(()), b"\xf6");
    golden(&Some(Box::new([1u8, 2])), b"\x82\x01\x02");
    golden(&None::<Box<u8>>, b"\xf6");
    golden(
        &Boxed {
            id: 1,
            point: Box::new((2, "z".into())),
            unit: Box::new(()),
            nested: Some(Box::new([Box::new(3), Box::new(4)])),
        },
        b"\xa4\x62id\x01\x65point\x82\x02\x61z\x64unit\xf6\x66nested\x82\x03\x04",
    );
}

#[test]
fn a_missing_boxed_option_is_a_missing_field() {
    #[derive(Cbor, Debug)]
    struct Holder {
        id: u8,
        detail: Box<Option<u8>>,
    }

    assert_eq!(
        from_slice::<Holder>(b"\xa1\x62id\x01")
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ErrorKind::MissingField)
    );
}

#[test]
fn a_box_fails_as_the_value_it_holds() {
    #[derive(Cbor, Debug)]
    struct Unboxed {
        id: u8,
        point: (u16, String),
        unit: (),
        nested: Option<[u8; 2]>,
    }

    for input in [
        b"\xa4\x62id\x01\x65point\x82\x02\x62a\xff".as_slice(),
        b"\xa2\x62id\x01\x65point\x82\x02\xf4",
        b"\xa4\x62id\x01\x65point\x82\x02\x60\x64unit\xf6\x66nested\x81\x01\x02",
        b"\xa4\x62id\x01\x65point\x82\x02\x60\x64unit\xf6\x66nested\x82\x01",
    ] {
        assert_eq!(
            from_slice::<Boxed>(input)
                .map(|_| ())
                .map_err(|e| (e.kind, e.field, e.offset)),
            from_slice::<Unboxed>(input)
                .map(|_| ())
                .map_err(|e| (e.kind, e.field, e.offset)),
            "{input:x?}"
        );
    }
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 4 } else { 256 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn any_text_and_bytes_round_trip_in_any_chunks(
        tags in prop::collection::vec(".{0,8}", 0..4),
        blob in prop::collection::vec(any::<u8>(), 0..150),
    ) {
        let record = Record { id: 0, name: tags.concat(), tags, blob, note: None };
        golden(&record, &encode(&record));
    }
}
