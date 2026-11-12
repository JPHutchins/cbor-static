use core::cell::Cell;
use core::mem::MaybeUninit;
use std::vec::Vec;

use super::leaf::store;
use super::{Decoder, Encoder, Item, Next, drop_fn, index_tag};
use crate::error::{Error, ErrorKind};
use crate::scalar::Scalar;
use crate::schema::{DecKind, Decode, EncKind, Encode, Layout};

mod drops;

std::thread_local! {
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

struct Tracked;

impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.with(|drops| drops.set(drops.get() + 1));
    }
}

unsafe fn decode_tracked(dst: *mut u8, item: &Item<'_>) -> Result<Next, ErrorKind> {
    u32::decode(item)
        .map(|_| Tracked)
        .map(unsafe { store::<Tracked>(dst) })
}

unsafe impl Decode<'_> for Tracked {
    const DECODE: &'static DecKind = &DecKind::Leaf {
        decode: decode_tracked,
        default: None,
        drop: drop_fn::<Tracked>(),
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
    type OptionStack = crate::stack::Level<()>;
}

#[derive(crate::Decode)]
struct Pair {
    a: Tracked,
    b: Tracked,
    c: u32,
}

#[derive(crate::Decode)]
struct Outer {
    first: Pair,
    second: Pair,
}

#[derive(crate::Decode)]
#[cbor(array)]
struct Positional {
    a: Tracked,
    b: Tracked,
    c: u32,
}

#[derive(crate::Decode)]
struct Maybe {
    first: Option<Pair>,
    after: u32,
}

const MAYBE: &[u8] = b"\xa2\x65first\xa3\x61a\x01\x61b\x02\x61c\x03\x65after\x07";
/// Where `MAYBE` holds the values of `a`, `b`, `c` and `after`, and the drops failing there.
const MAYBE_DROPS: [(usize, usize); 4] = [(10, 0), (13, 1), (16, 2), (23, 2)];

#[derive(crate::Decode)]
#[serde(untagged)]
enum Choice {
    Count(u32),
    Pair(Pair),
}

const CHOICE: &[u8] = b"\xa3\x61a\x01\x61b\x02\x61c\x03";
/// Where `CHOICE` holds the values of `a`, `b` and `c`, and the drops failing there.
const CHOICE_DROPS: [(usize, usize); 3] = [(3, 0), (6, 1), (9, 2)];

#[derive(crate::Decode)]
#[cbor(index)]
enum Indexed {
    Unit,
    Pair(Pair),
}

#[derive(crate::Decode)]
enum CborTagged {
    #[cbor(cbor_tag = 5)]
    Pair(Pair),
}

#[derive(crate::Decode)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Pair(Pair),
}

#[derive(crate::Decode)]
#[serde(tag = "t")]
enum Internal {
    Pair(Pair),
}

/// `CHOICE`'s entries after an internal tag entry, which moves each value 7 bytes on.
const INTERNAL_PAIR: &[u8] = b"\xa4\x61t\x64Pair\x61a\x01\x61b\x02\x61c\x03";

/// `Positional`, `(Tracked, Tracked, u32)` and `[Tracked; 3]` alike: elements 1 and 2 decode to
/// `Tracked`.
const POSITIONAL: &[u8] = b"\x83\x01\x02\x03";

const COMPLETE: &[u8] =
    b"\xa2\x65first\xa3\x61a\x01\x61b\x02\x61c\x03\x66second\xa3\x61a\x04\x61b\x05\x61c\x06";
const DUPLICATE_A: &[u8] = b"\xa2\x65first\xa3\x61a\x01\x61a\x02";
const DROPS_BEFORE_EACH_VALUE: [usize; 8] = [0, 0, 1, 2, 2, 2, 3, 4];

/// Where each value in `COMPLETE` starts, each `Pair` and its fields alike. Every value there is
/// one byte, so the next key follows it.
fn value_offsets() -> impl Iterator<Item = usize> {
    core::iter::successors(Some(after_key(1)), |&value| {
        Some(value + 1)
            .filter(|&key| key < COMPLETE.len())
            .map(after_key)
    })
}

fn after_key(key: usize) -> usize {
    key + 1 + usize::from(COMPLETE[key] & 0x1f)
}

fn drops_after<T: for<'de> Decode<'de>>(bytes: &[u8], chunk: usize) -> (Option<ErrorKind>, usize) {
    DROPS.with(|drops| drops.set(0));
    let mut slot = MaybeUninit::<T>::uninit();
    let mut decoder = Decoder::new(&mut slot);
    let error = bytes
        .chunks(chunk)
        .find_map(|piece| decoder.feed(piece).err())
        .map(|e| e.kind);
    drop(decoder);
    (error, DROPS.with(Cell::get))
}

#[test]
fn an_error_drops_exactly_the_initialized_fields() {
    for chunk in [1, 7, 64] {
        assert_eq!(
            value_offsets()
                .map(|value| drops_after::<Outer>(&[&COMPLETE[..value], b"\xf5"].concat(), chunk))
                .collect::<Vec<_>>(),
            DROPS_BEFORE_EACH_VALUE.map(|drops| (Some(ErrorKind::TypeMismatch), drops))
        );
        assert_eq!(
            drops_after::<Outer>(DUPLICATE_A, chunk),
            (Some(ErrorKind::DuplicateKey), 1)
        );
    }
}

#[test]
fn an_abandoned_decoder_drops_what_it_initialized() {
    for chunk in [1, 7, 64] {
        assert_eq!(
            value_offsets()
                .map(|value| drops_after::<Outer>(&COMPLETE[..value], chunk))
                .collect::<Vec<_>>(),
            DROPS_BEFORE_EACH_VALUE.map(|drops| (None, drops))
        );
        assert_eq!(drops_after::<Outer>(COMPLETE, chunk), (None, 4));
    }
}

#[test]
fn positional_forms_drop_exactly_the_initialized_elements() {
    for chunk in [1, 2, 64] {
        for (element, drops) in [(1, 0), (2, 1), (3, 2)] {
            let failing = [&POSITIONAL[..element], b"\xf5"].concat();
            for outcome in [
                drops_after::<Positional>(&failing, chunk),
                drops_after::<(Tracked, Tracked, u32)>(&failing, chunk),
                drops_after::<[Tracked; 3]>(&failing, chunk),
            ] {
                assert_eq!(outcome, (Some(ErrorKind::TypeMismatch), drops));
            }
            for outcome in [
                drops_after::<Positional>(&POSITIONAL[..element], chunk),
                drops_after::<(Tracked, Tracked, u32)>(&POSITIONAL[..element], chunk),
                drops_after::<[Tracked; 3]>(&POSITIONAL[..element], chunk),
            ] {
                assert_eq!(outcome, (None, drops));
            }
        }
    }
}

#[test]
fn an_option_drops_its_payload_pending_or_moved_into_some() {
    for chunk in [1, 5, 64] {
        for (value, drops) in MAYBE_DROPS {
            assert_eq!(
                drops_after::<Maybe>(&[&MAYBE[..value], b"\xf5"].concat(), chunk),
                (Some(ErrorKind::TypeMismatch), drops)
            );
            assert_eq!(drops_after::<Maybe>(&MAYBE[..value], chunk), (None, drops));
        }
        assert_eq!(drops_after::<Maybe>(MAYBE, chunk), (None, 2));
    }
}

/// Failing or abandoning `T`'s decoding of `complete` at each value drops exactly the given count.
fn drops_at<T: for<'de> Decode<'de>>(complete: &[u8], values: [(usize, usize); 3], chunk: usize) {
    for (at, drops) in values {
        assert_eq!(
            drops_after::<T>(&[&complete[..at], b"\xf5"].concat(), chunk),
            (Some(ErrorKind::TypeMismatch), drops)
        );
        assert_eq!(drops_after::<T>(&complete[..at], chunk), (None, drops));
    }
}

/// `drops_at` for `CHOICE` decoded as a `Pair` payload after `prefix`.
fn drops_inside_payload<T: for<'de> Decode<'de>>(prefix: &[u8], chunk: usize) {
    drops_at::<T>(
        &[prefix, CHOICE].concat(),
        CHOICE_DROPS.map(|(value, drops)| (prefix.len() + value, drops)),
        chunk,
    );
}

#[test]
fn enum_variants_drop_what_their_payload_initialized() {
    for chunk in [1, 4, 64] {
        drops_inside_payload::<Choice>(b"", chunk);
        drops_inside_payload::<Indexed>(b"\x82\x01", chunk);
        drops_inside_payload::<CborTagged>(b"\xc5", chunk);
        assert_eq!(drops_after::<Choice>(CHOICE, chunk), (None, 2));
        if chunk >= INTERNAL_PAIR.len() {
            drops_inside_payload::<Adjacent>(b"\xa2\x61t\x64Pair\x61c", chunk);
            drops_at::<Internal>(
                INTERNAL_PAIR,
                CHOICE_DROPS.map(|(value, drops)| (value + 7, drops)),
                chunk,
            );
        }
        assert_eq!(
            drops_after::<Indexed>(&[b"\x83\x01".as_slice(), CHOICE, b"\x00"].concat(), chunk),
            (Some(ErrorKind::TypeMismatch), 2)
        );
    }
}

#[test]
fn a_finished_value_belongs_to_the_caller() {
    DROPS.with(|drops| drops.set(0));
    let mut slot = MaybeUninit::<Outer>::uninit();
    let mut decoder = Decoder::new(&mut slot);
    assert!(decoder.feed(COMPLETE).is_ok());
    assert!(decoder.finish().is_ok());
    drop(decoder);
    assert_eq!(DROPS.with(Cell::get), 0);
    unsafe { slot.assume_init_drop() };
    assert_eq!(DROPS.with(Cell::get), 4);
}

#[repr(u8)]
enum Unlisted {
    Variant,
}

unsafe impl Encode for Unlisted {
    const ENCODE: &'static EncKind = &EncKind::Enum {
        ops: &super::ENCODE_EXTERNAL,
        layout: &Layout {
            names: b"",
            fields: &[],
        },
        payloads: &[],
        index: index_tag,
        field_ptr: None,
    };
    type Stack = ();
    const MAX_DEPTH: usize = 0;
}

#[derive(crate::Encode)]
struct Carrier {
    a: u32,
    b: Unlisted,
}

#[test]
fn an_encode_error_reports_its_field_and_its_offset_across_buffers() {
    for chunk in [1, 7, 64] {
        let mut encoder = Encoder::new(&Carrier {
            a: 1,
            b: Unlisted::Variant,
        });
        assert_eq!(
            [0u8; 64]
                .chunks_mut(chunk)
                .find_map(|piece| encoder.resume(piece).err()),
            Some(Error::new(ErrorKind::UnknownVariant, 1, 6))
        );
    }
}
