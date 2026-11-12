//! heapless `Vec` and `LinearMap` (#12 Q19): arrays and maps of up to a capacity, decoded in place
//! chunk by chunk, whose keys differ, and which drop exactly what was decoded when decoding stops
//! (D-17).
#![cfg(feature = "heapless")]

mod common;

use core::cell::Cell;
use core::mem::MaybeUninit;

use cbor_static::{Cbor, ErrorKind};
use common::golden_in_place;
use heapless::{LinearMap, String, Vec};

#[derive(Cbor, Clone, Copy, Debug, PartialEq)]
struct Point {
    x: u32,
    y: u32,
}

fn vec<T: Clone, const N: usize>(items: &[T]) -> Vec<T, N> {
    Vec::from_slice(items).expect("fits")
}

fn map<K: Eq, V, const N: usize>(entries: impl IntoIterator<Item = (K, V)>) -> LinearMap<K, V, N> {
    entries.into_iter().collect()
}

fn decoded<T>(bytes: &[u8]) -> Result<(), ErrorKind>
where
    T: for<'de> cbor_static::Decode<'de>,
{
    let mut slot = MaybeUninit::<T>::uninit();
    cbor_static::decode_into(bytes, &mut slot)
        .map(|_| ())
        .map_err(|e| e.kind)
}

#[test]
fn a_vec_is_an_array_of_up_to_its_capacity() {
    golden_in_place(&vec::<u32, 4>(&[1, 2, 1000]), b"\x83\x01\x02\x19\x03\xe8");
    golden_in_place(&vec::<u32, 4>(&[]), b"\x80");
    golden_in_place(&vec::<u8, 4>(&[1, 2]), b"\x82\x01\x02");
    golden_in_place(
        &vec::<Point, 2>(&[Point { x: 1, y: 2 }]),
        b"\x81\xa2\x61x\x01\x61y\x02",
    );
    golden_in_place(
        &vec::<Vec<u8, 2>, 2>(&[vec(&[1]), vec(&[2, 3])]),
        b"\x82\x81\x01\x82\x02\x03",
    );
    golden_in_place(
        &vec::<String<2>, 2>(&[String::try_from("ab").expect("fits")]),
        b"\x81\x62ab",
    );
    assert_eq!(
        decoded::<Vec<u32, 2>>(b"\x83\x01\x02\x03"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        decoded::<Vec<u32, 2>>(b"\x9f\x01\x02\x03\xff"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        decoded::<Vec<u32, 2>>(b"\xa0"),
        Err(ErrorKind::TypeMismatch)
    );
    assert_eq!(decoded::<Vec<u32, 2, u8>>(b"\x9f\x01\x02\xff"), Ok(()));
}

#[test]
fn a_linear_map_is_a_map_of_distinct_keys_in_input_order() {
    golden_in_place(
        &map::<u8, bool, 4>([(2, true), (1, false)]),
        b"\xa2\x02\xf5\x01\xf4",
    );
    golden_in_place(&map::<u8, bool, 4>([]), b"\xa0");
    golden_in_place(
        &map::<String<4>, Vec<u8, 2>, 2>([(String::try_from("ab").expect("fits"), vec(&[1]))]),
        b"\xa1\x62ab\x81\x01",
    );
    assert_eq!(
        decoded::<LinearMap<u8, bool, 4>>(b"\xbf\x02\xf5\x01\xf4\xff"),
        Ok(())
    );
    assert_eq!(
        decoded::<LinearMap<u8, bool, 4>>(b"\xa2\x01\xf5\x01\xf4"),
        Err(ErrorKind::DuplicateKey)
    );
    assert_eq!(
        decoded::<LinearMap<String<4>, u8, 4>>(b"\xa2\x61a\x01\x7f\x61a\xff\x02"),
        Err(ErrorKind::DuplicateKey)
    );
    assert_eq!(
        decoded::<LinearMap<u8, bool, 1>>(b"\xa2\x01\xf5\x02\xf4"),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        decoded::<LinearMap<u8, bool, 4>>(b"\xbf\x01\xf5\x02\xff"),
        Err(ErrorKind::NotWellFormed)
    );
    assert_eq!(
        decoded::<LinearMap<u8, bool, 4>>(b"\x80"),
        Err(ErrorKind::TypeMismatch)
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

/// How many `Counted` values decoding `bytes` as `T` dropped, which must fail.
fn dropped_on_failure<T>(bytes: &[u8]) -> usize
where
    T: for<'de> cbor_static::Decode<'de>,
{
    DROPPED.set(0);
    assert!(decoded::<T>(bytes).is_err(), "{bytes:x?}");
    DROPPED.get()
}

#[test]
fn a_failed_decode_drops_exactly_the_elements_and_keys_decoded() {
    assert_eq!(
        dropped_on_failure::<Vec<Counted, 4>>(b"\x83\x01\x02\xf6"),
        2
    );
    assert_eq!(
        dropped_on_failure::<Vec<Counted, 4>>(b"\x9f\x01\x02\xf6\xff"),
        2
    );
    assert_eq!(dropped_on_failure::<Vec<Counted, 1>>(b"\x82\x01\x02"), 1);
    assert_eq!(
        dropped_on_failure::<LinearMap<u8, Counted, 4>>(b"\xa3\x01\x01\x02\x02\x03\xf6"),
        2
    );
    assert_eq!(
        dropped_on_failure::<LinearMap<Counted, u8, 4>>(b"\xa2\x01\x01\x02\xf6"),
        2
    );
    assert_eq!(
        dropped_on_failure::<LinearMap<Counted, u8, 4>>(b"\xa2\x01\x01\x01\x02"),
        2
    );
}

#[test]
fn a_whole_map_drops_once_as_a_map() {
    DROPPED.set(0);
    let mut slot = MaybeUninit::<LinearMap<u8, Counted, 4>>::uninit();
    let mut decoder = cbor_static::Decoder::new(&mut slot);
    assert_eq!(
        decoder.feed(b"\xa2\x01\x01\x02\x02").map_err(|e| e.kind),
        Ok(cbor_static::Fed::Done(5))
    );
    drop(decoder);
    assert_eq!(DROPPED.get(), 2);
}
