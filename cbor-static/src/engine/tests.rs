use core::cell::Cell;
use core::mem::MaybeUninit;
use std::vec::Vec;

use super::leaf::store;
use super::{Decoder, Encoder, Item, Next, drop_fn, index_tag};
use crate::error::{Error, ErrorKind};
use crate::scalar::Scalar;
use crate::schema::{DecKind, Decode, EncKind, Encode, Layout};

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

fn drops_after(bytes: &[u8], chunk: usize) -> (Option<ErrorKind>, usize) {
    DROPS.with(|drops| drops.set(0));
    let mut slot = MaybeUninit::<Outer>::uninit();
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
                .map(|value| drops_after(&[&COMPLETE[..value], b"\xf5"].concat(), chunk))
                .collect::<Vec<_>>(),
            DROPS_BEFORE_EACH_VALUE.map(|drops| (Some(ErrorKind::TypeMismatch), drops))
        );
        assert_eq!(
            drops_after(DUPLICATE_A, chunk),
            (Some(ErrorKind::DuplicateKey), 1)
        );
    }
}

#[test]
fn an_abandoned_decoder_drops_what_it_initialized() {
    for chunk in [1, 7, 64] {
        assert_eq!(
            value_offsets()
                .map(|value| drops_after(&COMPLETE[..value], chunk))
                .collect::<Vec<_>>(),
            DROPS_BEFORE_EACH_VALUE.map(|drops| (None, drops))
        );
        assert_eq!(drops_after(COMPLETE, chunk), (None, 4));
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
