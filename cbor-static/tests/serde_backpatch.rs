#![cfg(all(feature = "serde", feature = "alloc"))]
//! Sequences, maps and text of unknown length are written definite and preferred (#6 Q6): their
//! one-byte head is patched when they close, moving their contents when the count needs a longer
//! head. A declared length must be the number of items written (#57).

use std::collections::BTreeMap;

use cbor_static::ErrorKind;
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

/// Its items, as a sequence of unknown length.
struct Unsized<T>(Vec<T>);

impl<T: Serialize> Serialize for Unsized<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().filter(|_| true))
    }
}

/// Its entries, as a map of unknown length.
struct UnsizedMap(BTreeMap<u32, u8>);

impl Serialize for UnsizedMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().filter(|_| true))
    }
}

/// Its text, through `collect_str`.
struct Displayed(String);

impl Serialize for Displayed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

/// The longest encoding every buffer size short of it is checked for.
const SWEPT: usize = if cfg!(miri) { 64 } else { 1024 };

/// `unknown` is written as `known` is, through `to_vec` and `to_slice`, and fails where `known`
/// fails in a buffer of every size short of it; past [`SWEPT`] bytes, of the sizes short of the
/// head, of the last bytes, where its contents move, and of every 4,096th.
fn as_known(unknown: &impl Serialize, known: &impl Serialize) {
    let bytes = cbor_static::serde::to_vec(known).expect("known");
    assert_eq!(cbor_static::serde::to_vec(unknown).as_ref(), Ok(&bytes));
    let mut out = vec![0u8; bytes.len()];
    assert_eq!(
        cbor_static::serde::to_slice(unknown, &mut out),
        Ok(bytes.len())
    );
    assert_eq!(out, bytes);
    for room in (0..bytes.len()).filter(|&room| {
        bytes.len() <= SWEPT || room < 4 || bytes.len() - room < 12 || room % 4096 == 0
    }) {
        assert_eq!(
            cbor_static::serde::to_slice(unknown, &mut vec![0u8; room])
                .map_err(|e| (e.kind, e.offset)),
            cbor_static::serde::to_slice(known, &mut vec![0u8; room])
                .map_err(|e| (e.kind, e.offset)),
            "{room} of {len} bytes",
            len = bytes.len()
        );
    }
}

/// Counts either side of the limits of 1-, 2- and 3-byte heads.
const BOUNDARIES: &[usize] = if cfg!(miri) {
    &[0, 1, 23, 24, 255, 256]
} else {
    &[0, 1, 23, 24, 255, 256, 65_535, 65_536]
};

#[test]
fn a_sequence_of_unknown_length_takes_the_head_its_count_needs() {
    for &count in BOUNDARIES {
        let items = (0..count)
            .map(|item| u8::try_from(item % 251).expect("below 251"))
            .collect::<Vec<_>>();
        as_known(&Unsized(items.clone()), &items);
    }
}

#[test]
fn a_map_of_unknown_length_takes_the_head_its_count_needs() {
    for &count in BOUNDARIES {
        let entries = (0..u32::try_from(count).expect("count"))
            .map(|key| (key, u8::try_from(key % 251).expect("below 251")))
            .collect::<BTreeMap<_, _>>();
        as_known(&UnsizedMap(entries.clone()), &entries);
    }
}

#[test]
fn collected_text_takes_the_head_its_length_needs() {
    for &len in BOUNDARIES {
        let text = "é".repeat(len / 2) + &"a".repeat(len % 2);
        as_known(&Displayed(text.clone()), &text);
    }
}

#[test]
fn nested_unknown_lengths_each_take_their_own_head() {
    let inner = |count: usize| {
        (0..count)
            .map(|item| u32::try_from(item).expect("count") * 300)
            .collect::<Vec<_>>()
    };
    let nested = [0, 24, 300, 3].map(inner);
    as_known(
        &Unsized(nested.iter().cloned().map(Unsized).collect()),
        &nested,
    );
    let deep = vec![vec![vec![1u8; 30]; 25]; 2];
    as_known(
        &Unsized(
            deep.iter()
                .map(|middle| Unsized(middle.iter().cloned().map(Unsized).collect()))
                .collect(),
        ),
        &deep,
    );
}

#[test]
fn a_full_buffer_while_moving_is_at_its_end() {
    let items = vec![7u8; 24];
    assert_eq!(
        cbor_static::serde::to_slice(&Unsized(items), &mut [0u8; 25])
            .map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::BufferTooSmall, 25))
    );
}

/// A sequence or map that declares `declared` items and writes `written`.
struct Lying {
    map: bool,
    declared: usize,
    written: usize,
}

impl Serialize for Lying {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.map {
            let mut map = serializer.serialize_map(Some(self.declared))?;
            (0..self.written).try_for_each(|key| map.serialize_entry(&key, &0u8))?;
            map.end()
        } else {
            let mut seq = serializer.serialize_seq(Some(self.declared))?;
            (0..self.written).try_for_each(|_| seq.serialize_element(&0u8))?;
            seq.end()
        }
    }
}

#[test]
fn more_items_than_declared_are_refused_before_the_first_extra_one() {
    let refused = |map, declared, written| {
        cbor_static::serde::to_vec(&Lying {
            map,
            declared,
            written,
        })
        .map_err(|e| (e.kind, e.offset))
    };
    let past =
        |declared: usize, item: &[u8]| u32::try_from(1 + declared * item.len()).expect("short");
    assert_eq!(
        refused(false, 2, 3),
        Err((ErrorKind::NotWellFormed, past(2, b"\x00")))
    );
    assert_eq!(
        refused(true, 2, 3),
        Err((ErrorKind::NotWellFormed, past(2, b"\x00\x00")))
    );
    assert_eq!(
        refused(false, 0, 1),
        Err((ErrorKind::NotWellFormed, past(0, b"\x00")))
    );
}

#[test]
fn fewer_items_than_declared_are_refused_where_they_end() {
    let refused = |map, declared, written| {
        cbor_static::serde::to_slice(
            &Lying {
                map,
                declared,
                written,
            },
            &mut [0u8; 16],
        )
        .map_err(|e| (e.kind, e.offset))
    };
    let end = |written: usize, item: &[u8]| u32::try_from(1 + written * item.len()).expect("short");
    assert_eq!(
        refused(false, 3, 2),
        Err((ErrorKind::NotWellFormed, end(2, b"\x00")))
    );
    assert_eq!(
        refused(true, 3, 2),
        Err((ErrorKind::NotWellFormed, end(2, b"\x00\x00")))
    );
    assert_eq!(
        refused(false, 1, 0),
        Err((ErrorKind::NotWellFormed, end(0, b"\x00")))
    );
}

#[test]
fn nothing_past_the_declared_count_lands_in_the_buffer() {
    for map in [false, true] {
        let mut out = [0xaa; 16];
        let lying = Lying {
            map,
            declared: 2,
            written: 3,
        };
        let declared_end = if map { 1 + 2 * 2 } else { 1 + 2 };
        assert_eq!(
            cbor_static::serde::to_slice(&lying, &mut out).map_err(|e| (e.kind, e.offset)),
            Err((
                ErrorKind::NotWellFormed,
                u32::try_from(declared_end).expect("short")
            ))
        );
        assert!(
            out[declared_end..].iter().all(|&byte| byte == 0xaa),
            "{out:02x?}"
        );
    }
}

/// A struct or tuple that declares `declared` fields and writes `written`.
struct LyingFields {
    tuple: bool,
    declared: usize,
    written: usize,
}

impl Serialize for LyingFields {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeStruct, SerializeTuple};

        const NAMES: [&str; 4] = ["a", "b", "c", "d"];
        if self.tuple {
            let mut tuple = serializer.serialize_tuple(self.declared)?;
            (0..self.written).try_for_each(|_| tuple.serialize_element(&0u8))?;
            tuple.end()
        } else {
            let mut fields = serializer.serialize_struct("Lying", self.declared)?;
            NAMES[..self.written]
                .iter()
                .try_for_each(|&name| fields.serialize_field(name, &0u8))?;
            fields.end()
        }
    }
}

#[test]
fn a_struct_or_tuple_of_the_wrong_length_is_refused() {
    let refused = |tuple, declared, written| {
        cbor_static::serde::to_slice(
            &LyingFields {
                tuple,
                declared,
                written,
            },
            &mut [0u8; 16],
        )
        .map_err(|e| (e.kind, e.offset))
    };
    let field = b"\x61a\x00".len();
    let past = |items: usize, item: usize| u32::try_from(1 + items * item).expect("short");
    assert_eq!(
        refused(false, 2, 3),
        Err((ErrorKind::NotWellFormed, past(2, field)))
    );
    assert_eq!(
        refused(false, 2, 1),
        Err((ErrorKind::NotWellFormed, past(1, field)))
    );
    assert_eq!(
        refused(true, 2, 3),
        Err((ErrorKind::NotWellFormed, past(2, 1)))
    );
    assert_eq!(
        refused(true, 2, 1),
        Err((ErrorKind::NotWellFormed, past(1, 1)))
    );
    assert_eq!(refused(true, 2, 2).map(|_| ()), Ok(()));
}

#[test]
fn a_declared_length_written_in_full_is_unchanged() {
    let exact = Lying {
        map: true,
        declared: 2,
        written: 2,
    };
    assert_eq!(
        cbor_static::serde::to_vec(&exact).as_deref(),
        Ok(&b"\xa2\x00\x00\x01\x00"[..])
    );
}

#[derive(Serialize, cbor_static::Cbor)]
struct Inner {
    x: u8,
    y: u8,
    z: u8,
    w: u8,
}

/// Twenty-four entries with its flattened field's: `serde_derive` writes it as a map of unknown
/// length, the native derive as a definite one.
#[derive(Serialize, cbor_static::Cbor)]
struct Wide {
    f00: u16,
    f01: u16,
    f02: u16,
    f03: u16,
    f04: u16,
    f05: u16,
    f06: u16,
    f07: u16,
    f08: u16,
    f09: u16,
    f10: u16,
    f11: u16,
    f12: u16,
    f13: u16,
    f14: u16,
    f15: u16,
    f16: u16,
    f17: u16,
    f18: u16,
    f19: u16,
    #[serde(flatten)]
    inner: Inner,
}

#[test]
fn a_flattened_struct_is_the_native_derive_bytes_at_every_buffer_size() {
    let wide = Wide {
        f00: 0,
        f01: 1000,
        f02: 2000,
        f03: 3000,
        f04: 4000,
        f05: 5000,
        f06: 6000,
        f07: 7000,
        f08: 8000,
        f09: 9000,
        f10: 10000,
        f11: 11000,
        f12: 12000,
        f13: 13000,
        f14: 14000,
        f15: 15000,
        f16: 16000,
        f17: 17000,
        f18: 18000,
        f19: 19000,
        inner: Inner {
            x: 1,
            y: 2,
            z: 3,
            w: 4,
        },
    };
    let native = cbor_static::to_vec(&wide).expect("native");
    assert_eq!(native.get(..2), Some(&b"\xb8\x18"[..]));
    assert_eq!(cbor_static::serde::to_vec(&wide).as_ref(), Ok(&native));
    for room in 0..=native.len() {
        assert_eq!(
            cbor_static::serde::to_slice(&wide, &mut vec![0u8; room])
                .map_err(|e| (e.kind, e.offset)),
            cbor_static::to_slice(&wide, &mut vec![0u8; room]).map_err(|e| (e.kind, e.offset)),
            "{room} of {len} bytes",
            len = native.len()
        );
    }
}

/// A `Display` impl that ignores a write it could not make, then makes a shorter one.
struct Careless;

impl core::fmt::Display for Careless {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = formatter.write_str("abcdef");
        formatter.write_str("x")
    }
}

#[test]
fn collected_text_that_did_not_fit_stays_refused() {
    struct Shown;

    impl Serialize for Shown {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.collect_str(&Careless)
        }
    }

    assert_eq!(
        cbor_static::serde::to_slice(&Shown, &mut [0u8; 4]).map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::BufferTooSmall, 4))
    );
    assert_eq!(
        cbor_static::serde::to_vec(&Shown).as_deref(),
        Ok(&b"\x67abcdefx"[..])
    );
}
