//! #5 Q9, Q10: decoding reserves only what the input has delivered, never what a header declares,
//! and a reservation the allocator refuses is `OutOfMemory`, leaving nothing allocated. The global
//! allocator counts each thread's live bytes and largest block, and refuses past a thread's budget;
//! a block freed on another thread than allocated it wraps both counts alike.
#![cfg(feature = "alloc")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::mem::MaybeUninit;

use cbor_static::{Cbor, Decoder, ErrorKind, Fed, from_slice};

struct Counting;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static LARGEST: Cell<usize> = const { Cell::new(0) };
    static BUDGET: Cell<usize> = const { Cell::new(usize::MAX) };
}

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE
            .try_with(Cell::get)
            .unwrap_or(0)
            .wrapping_add(layout.size());
        if live > BUDGET.try_with(Cell::get).unwrap_or(usize::MAX) {
            return std::ptr::null_mut();
        }
        let _ = LIVE.try_with(|cell| cell.set(live));
        let _ = LARGEST.try_with(|cell| cell.set(cell.get().max(layout.size())));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = LIVE.try_with(|cell| cell.set(cell.get().wrapping_sub(layout.size())));
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// The live bytes after `run`, and the largest block it allocated, under `budget` live bytes.
fn measured(budget: usize, run: impl FnOnce()) -> (usize, usize) {
    let before = LIVE.with(Cell::get);
    LARGEST.with(|cell| cell.set(0));
    BUDGET.with(|cell| cell.set(before.saturating_add(budget)));
    run();
    BUDGET.with(|cell| cell.set(usize::MAX));
    (
        LIVE.with(Cell::get).wrapping_sub(before),
        LARGEST.with(Cell::get),
    )
}

/// What a decoder of `T` makes of `input`, dropped before returning.
fn fed<T: for<'de> cbor_static::Decode<'de>>(input: &[u8]) -> Result<Fed, ErrorKind> {
    let mut slot = MaybeUninit::<T>::uninit();
    Decoder::new(&mut slot).feed(input).map_err(|e| e.kind)
}

/// The capacity `Vec` gives its first allocation of `T` (alloc's `RawVec::MIN_NON_ZERO_CAP`).
const fn min_non_zero_cap<T>() -> usize {
    match size_of::<T>() {
        1 => 8,
        2..=1024 => 4,
        _ => 1,
    }
}

const HUGE_ARRAY: &[u8] = b"\x9b\x00\x00\x00\x01\x00\x00\x00\x00";
const HUGE_TEXT: &[u8] = b"\x7b\x00\x00\x00\x01\x00\x00\x00\x00";
const HUGE_BYTES: &[u8] = b"\x5b\x00\x00\x00\x01\x00\x00\x00\x00";

#[test]
fn a_huge_array_header_reserves_only_slots_its_elements_reached() {
    for completed in 0..40 {
        let input = [HUGE_ARRAY, &[0u8; 40][..completed]].concat();
        let (live, largest) = measured(usize::MAX, || {
            assert_eq!(fed::<Vec<u32>>(&input), Ok(Fed::NeedInput));
        });
        assert_eq!(live, 0, "{completed} elements");
        assert!(
            largest / size_of::<u32>() <= min_non_zero_cap::<u32>().max(2 * (completed + 1)),
            "{completed} elements: {largest} bytes"
        );
    }
}

#[test]
fn a_huge_byte_string_header_reserves_only_what_arrived() {
    #[derive(Cbor, Debug)]
    struct Blob {
        #[cbor(bytes)]
        blob: Vec<u8>,
    }

    for delivered in 0..40 {
        let input = [
            b"\xa1\x64blob".as_slice(),
            HUGE_BYTES,
            &[0u8; 40][..delivered],
        ]
        .concat();
        let (live, largest) = measured(usize::MAX, || {
            assert_eq!(fed::<Blob>(&input), Ok(Fed::NeedInput));
        });
        assert_eq!(live, 0, "{delivered} bytes");
        assert!(
            largest <= min_non_zero_cap::<u8>().max(2 * delivered),
            "{delivered} bytes: {largest} reserved"
        );
    }
}

#[test]
fn a_huge_string_header_reserves_only_what_arrived() {
    for delivered in 0..40 {
        let input = [HUGE_TEXT, &b"a".repeat(40)[..delivered]].concat();
        let (live, largest) = measured(usize::MAX, || {
            assert_eq!(fed::<String>(&input), Ok(Fed::NeedInput));
        });
        assert_eq!(live, 0, "{delivered} bytes");
        assert!(
            largest <= min_non_zero_cap::<u8>().max(2 * delivered),
            "{delivered} bytes: {largest} reserved"
        );
    }
}

#[test]
fn max_len_rejects_a_header_before_reserving() {
    #[derive(Cbor, Debug)]
    struct Bounded {
        #[cbor(max_len = 4)]
        name: String,
        #[cbor(max_len = 4)]
        values: Vec<u32>,
    }

    for input in [
        [b"\xa1\x64name".as_slice(), HUGE_TEXT].concat(),
        [b"\xa1\x66values".as_slice(), HUGE_ARRAY].concat(),
    ] {
        assert_eq!(
            measured(0, || {
                assert_eq!(fed::<Bounded>(&input), Err(ErrorKind::LimitExceeded));
            }),
            (0, 0)
        );
    }
}

#[test]
fn a_refused_reservation_is_out_of_memory_and_frees_the_rest() {
    #[derive(Cbor, Debug)]
    struct Record {
        name: String,
        values: Vec<String>,
        #[cbor(bytes)]
        blob: Vec<u8>,
        point: Box<(u8, Box<u8>)>,
    }

    let input =
        b"\xa4\x64name\x62ab\x66values\x82\x61x\x61y\x64blob\x43\x01\x02\x03\x65point\x82\x01\x02";
    let (live, needed) = measured(usize::MAX, || {
        assert!(from_slice::<Record>(input).is_ok());
    });
    assert_eq!(live, 0);
    for end in 0..input.len() {
        let (live, _) = measured(usize::MAX, || {
            assert!(from_slice::<Record>(&input[..end]).is_err());
            let mut wrong = input.to_vec();
            wrong[end] = 0xf4;
            drop(from_slice::<Record>(&wrong));
        });
        assert_eq!(live, 0, "{end} bytes");
    }
    let outcomes: Vec<_> = (0..needed * 8)
        .map(|budget| {
            let outcome = Cell::new(Ok(()));
            let (live, _) = measured(budget, || {
                outcome.set(from_slice::<Record>(input).map(|_| ()).map_err(|e| e.kind));
            });
            assert_eq!(live, 0, "budget {budget}");
            outcome.get()
        })
        .collect();
    assert!(outcomes.contains(&Ok(())));
    assert!(outcomes.contains(&Err(ErrorKind::OutOfMemory)));
    assert!(
        outcomes
            .iter()
            .all(|outcome| matches!(outcome, Ok(()) | Err(ErrorKind::OutOfMemory)))
    );
    assert_eq!(
        measured(0, || {
            assert_eq!(
                from_slice::<Record>(input).map(|_| ()).map_err(|e| e.kind),
                Err(ErrorKind::OutOfMemory)
            );
        }),
        (0, 0)
    );
}
