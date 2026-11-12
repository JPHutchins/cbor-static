//! Struct shapes follow serde (D-08, #12 Q22): a unit struct and `()` are null, a newtype struct is
//! its field, and a tuple struct is an array of every field.

mod common;

use core::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

use cbor_static::{Cbor, Decoder, ErrorKind, Fed};
use common::{decode, encode, golden};

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Unit;

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Meters(u32);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Celsius(i16);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Wrapped(Meters);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Pair(u32, Option<u32>);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Empty();

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Point {
    x: u32,
    y: u32,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Located(Point);

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Reading {
    at: Meters,
    unit: Unit,
    pair: Pair,
    seen: Option<Meters>,
}

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
enum Either {
    Distance(Meters),
    Nothing(Unit),
}

#[test]
fn a_unit_struct_and_the_unit_type_are_null() {
    golden(&Unit, b"\xf6");
    golden(&(), b"\xf6");
    golden(&[(), ()], b"\x82\xf6\xf6");
    assert_eq!(decode::<Unit>(b"\xf7"), Err(ErrorKind::TypeMismatch));
    assert_eq!(decode::<()>(b"\x00"), Err(ErrorKind::TypeMismatch));
}

#[test]
fn an_option_of_a_unit_is_none_on_null_as_in_serde() {
    assert_eq!(encode(&Some(())), b"\xf6");
    assert_eq!(decode::<Option<()>>(b"\xf6"), Ok(None));
    assert_eq!(decode::<Option<Unit>>(b"\xf6"), Ok(None));
}

#[test]
fn a_newtype_struct_is_its_field() {
    golden(&Meters(1000), b"\x19\x03\xe8");
    golden(&Celsius(-40), b"\x38\x27");
    golden(&Wrapped(Meters(1)), b"\x01");
    golden(&Located(Point { x: 1, y: 2 }), b"\xa2\x61x\x01\x61y\x02");
    assert_eq!(encode(&Meters(7)), encode(&7u32));
    assert_eq!(
        decode::<Meters>(b"\x1b\0\0\0\x01\0\0\0\0"),
        Err(ErrorKind::OutOfRange)
    );
}

#[test]
fn a_tuple_struct_is_an_array_of_every_field() {
    golden(&Pair(1, None), b"\x82\x01\xf6");
    golden(&Pair(1, Some(2)), b"\x82\x01\x02");
    golden(&Empty(), b"\x80");
    assert_eq!(decode::<Pair>(b"\x81\x01"), Ok(Pair(1, None)));
    assert_eq!(decode::<Pair>(b"\x80"), Err(ErrorKind::MissingField));
}

#[test]
fn the_shapes_nest_as_fields_options_and_untagged_variants() {
    golden(
        &Reading {
            at: Meters(3),
            unit: Unit,
            pair: Pair(4, None),
            seen: Some(Meters(5)),
        },
        b"\xa4\x62at\x03\x64unit\xf6\x64pair\x82\x04\xf6\x64seen\x05",
    );
    golden(&Either::Distance(Meters(9)), b"\x09");
    golden(&Either::Nothing(Unit), b"\xf6");
}

static DROPPED: AtomicUsize = AtomicUsize::new(0);

#[derive(Cbor, Debug)]
struct Guard(u32);

impl Drop for Guard {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Cbor, Debug)]
struct Guarded {
    guard: Guard,
    after: u32,
}

/// A newtype reuses its field's descriptor but still drops as itself, with its own `Drop` (D-17).
#[test]
fn a_decoded_newtype_drops_as_itself() {
    DROPPED.store(0, Ordering::SeqCst);
    let mut slot = MaybeUninit::<Guarded>::uninit();
    assert_eq!(
        cbor_static::decode_into(b"\xa2\x65guard\x01\x65after\xf6", &mut slot)
            .map(|_| ())
            .map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(DROPPED.load(Ordering::SeqCst), 1);

    let mut slot = MaybeUninit::<Guard>::uninit();
    let mut decoder = Decoder::new(&mut slot);
    assert_eq!(decoder.feed(b"\x01").map_err(|e| e.kind), Ok(Fed::Done(1)));
    drop(decoder);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 2);
}
