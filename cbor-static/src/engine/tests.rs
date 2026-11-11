use core::cell::Cell;
use core::mem::MaybeUninit;

use super::leaf::store;
use super::{Decoder, Item, Next, drop_fn};
use crate::error::ErrorKind;
use crate::scalar::Scalar;
use crate::schema::{DecKind, Decode};

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

impl Decode<'_> for Tracked {
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
const SECOND_B_IS_BOOL: &[u8] =
    b"\xa2\x65first\xa3\x61a\x01\x61b\x02\x61c\x03\x66second\xa2\x61a\x04\x61b\xf5";
const DUPLICATE_A: &[u8] = b"\xa2\x65first\xa3\x61a\x01\x61a\x02";

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
            drops_after(SECOND_B_IS_BOOL, chunk),
            (Some(ErrorKind::TypeMismatch), 3)
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
            drops_after(&SECOND_B_IS_BOOL[..SECOND_B_IS_BOOL.len() - 2], chunk),
            (None, 3)
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
