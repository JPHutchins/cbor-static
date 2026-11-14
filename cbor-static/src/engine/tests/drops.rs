//! R-114 (D-17): a decode that fails, or is abandoned, at any item drops exactly the `Drop` fields
//! decoded before it, under either profile.

use core::cell::Cell;
use core::mem::MaybeUninit;
use std::vec::Vec;

use super::{DROPS, Tracked};
use crate::engine::{Chunks, Decoder};
use crate::error::ErrorKind;
use crate::schema::Decode;

#[derive(crate::Decode)]
struct Wide {
    a: Tracked,
    c: Tracked,
    d: [Tracked; 2],
    e: (Tracked, u32),
    f: Option<Tracked>,
    g: Tracked,
}

/// A map entry: its key, the items of its value, each marked when it decodes to a `Tracked`, and
/// whether its field has a default.
struct Entry {
    key: &'static [u8],
    items: &'static [(&'static [u8], bool)],
    default: bool,
}

/// `Wide`'s entries, in its keys' bytewise order.
const WIDE: [Entry; 6] = [
    Entry {
        key: b"\x61a",
        items: &[(b"\x01", true)],
        default: false,
    },
    Entry {
        key: b"\x61c",
        items: &[(b"\x02", true)],
        default: false,
    },
    Entry {
        key: b"\x61d",
        items: &[(b"\x82", false), (b"\x03", true), (b"\x04", true)],
        default: false,
    },
    Entry {
        key: b"\x61e",
        items: &[(b"\x82", false), (b"\x05", true), (b"\x06", false)],
        default: false,
    },
    Entry {
        key: b"\x61f",
        items: &[(b"\x07", true)],
        default: true,
    },
    Entry {
        key: b"\x61g",
        items: &[(b"\x08", true)],
        default: false,
    },
];

const CHUNKS: [usize; 4] = [1, 2, 5, usize::MAX];

#[derive(Clone, Copy)]
enum Profile {
    Default,
    Strict,
}

/// The error, if any, a decode stopped at, and the drops seen once its decoder is gone.
type Outcome = (Option<ErrorKind>, usize);

/// The [`Outcome`] of decoding `bytes` fed in `chunk`-byte pieces.
fn outcome<'de, T: Decode<'de>>(profile: Profile, bytes: &'de [u8], chunk: usize) -> Outcome {
    DROPS.with(|drops| drops.set(0));
    let mut slot = MaybeUninit::<T>::uninit();
    let mut decoder = match profile {
        Profile::Default => Decoder::new(&mut slot),
        Profile::Strict => Decoder::new_strict(&mut slot),
    };
    let error = bytes
        .chunks(chunk)
        .find_map(|piece| decoder.feed(piece).err())
        .map(|e| e.kind);
    drop(decoder);
    (error, DROPS.with(Cell::get))
}

/// A map of `entries`, with item `at` (entry, item) replaced by `with`.
fn map(entries: &[&Entry], at: Option<(usize, usize)>, with: &[u8]) -> Vec<u8> {
    let head = 0xa0 | u8::try_from(entries.len()).expect("a short map");
    entries
        .iter()
        .enumerate()
        .fold(Vec::from([head]), |bytes, (index, entry)| {
            entry.items.iter().enumerate().fold(
                [bytes.as_slice(), entry.key].concat(),
                |bytes, (item, &(encoded, _))| {
                    [
                        bytes.as_slice(),
                        if at == Some((index, item)) {
                            with
                        } else {
                            encoded
                        },
                    ]
                    .concat()
                },
            )
        })
}

/// The `Tracked` items in `entries` before item `at`.
fn tracked_before(entries: &[&Entry], (entry, item): (usize, usize)) -> usize {
    entries
        .iter()
        .enumerate()
        .flat_map(|(index, e)| {
            e.items
                .iter()
                .enumerate()
                .map(move |(position, &(_, tracked))| ((index, position), tracked))
        })
        .filter(|&(position, tracked)| tracked && position < (entry, item))
        .count()
}

fn every_item() -> impl Iterator<Item = (usize, usize)> {
    WIDE.iter()
        .enumerate()
        .flat_map(|(index, entry)| (0..entry.items.len()).map(move |item| (index, item)))
}

fn all() -> Vec<&'static Entry> {
    WIDE.iter().collect()
}

#[test]
fn a_mismatch_at_any_item_drops_what_came_before() {
    for profile in [Profile::Default, Profile::Strict] {
        for at in every_item() {
            let bytes = map(&all(), Some(at), b"\xf5");
            for chunk in CHUNKS {
                assert_eq!(
                    outcome::<Wide>(profile, &bytes, chunk),
                    (Some(ErrorKind::TypeMismatch), tracked_before(&all(), at)),
                    "{at:?} in chunks of {chunk}"
                );
            }
        }
    }
}

#[test]
fn input_cut_anywhere_drops_what_was_complete() {
    let bytes = map(&all(), None, b"");
    let ends: Vec<(usize, bool)> = WIDE
        .iter()
        .flat_map(|entry| {
            core::iter::once((entry.key.len(), false)).chain(
                entry
                    .items
                    .iter()
                    .map(|&(encoded, tracked)| (encoded.len(), tracked)),
            )
        })
        .scan(1, |at, (len, tracked)| {
            *at += len;
            Some((*at, tracked))
        })
        .collect();
    for cut in 0..=bytes.len() {
        let complete = ends
            .iter()
            .filter(|&&(end, tracked)| tracked && end <= cut)
            .count();
        for chunk in CHUNKS {
            assert_eq!(
                outcome::<Wide>(Profile::Default, &bytes[..cut], chunk),
                (None, complete),
                "cut at {cut} in chunks of {chunk}"
            );
        }
    }
}

/// Every entry of `Wide` in turn, with the entries before it decoding first.
fn every_entry() -> impl Iterator<Item = (usize, Vec<&'static Entry>)> {
    (0..WIDE.len()).map(|index| (index, all()))
}

#[test]
fn a_strict_rejection_at_any_key_drops_what_came_before() {
    let unknown = Entry {
        key: b"\x61z",
        items: &[(b"\x00", false)],
        default: true,
    };
    for (index, entries) in every_entry() {
        let before = tracked_before(&entries, (index, 0));
        let swapped: Vec<&Entry> = entries
            .iter()
            .enumerate()
            .map(|(at, entry)| if at == index { &unknown } else { entry })
            .collect();
        for chunk in CHUNKS {
            assert_eq!(
                outcome::<Wide>(Profile::Strict, &map(&swapped, None, b""), chunk),
                (Some(ErrorKind::UnknownField), before),
                "unknown at {index} in chunks of {chunk}"
            );
        }
        if index == 0 {
            continue;
        }
        let duplicated: Vec<&Entry> = entries
            .iter()
            .enumerate()
            .map(|(at, entry)| if at == index { entries[at - 1] } else { entry })
            .collect();
        let reordered: Vec<&Entry> = entries
            .iter()
            .enumerate()
            .map(|(at, _)| match at {
                at if at == index - 1 => entries[index],
                at if at == index => entries[index - 1],
                at => entries[at],
            })
            .collect();
        let moved_first = tracked_before(&entries, (index + 1, 0)) - before;
        for chunk in CHUNKS {
            assert_eq!(
                outcome::<Wide>(Profile::Strict, &map(&duplicated, None, b""), chunk),
                (Some(ErrorKind::DuplicateKey), before),
                "duplicate at {index} in chunks of {chunk}"
            );
            assert_eq!(
                outcome::<Wide>(Profile::Strict, &map(&reordered, None, b""), chunk),
                (
                    Some(ErrorKind::NotDeterministic),
                    tracked_before(&entries, (index - 1, 0)) + moved_first
                ),
                "out of order at {index} in chunks of {chunk}"
            );
            assert_eq!(
                outcome::<Wide>(Profile::Default, &map(&reordered, None, b""), chunk),
                (None, tracked_before(&entries, (entries.len(), 0))),
                "out of order is fine by default, and the abandoned value drops whole"
            );
        }
    }
}

#[test]
fn a_missing_field_drops_every_field_decoded() {
    for (index, entries) in every_entry() {
        let present: Vec<&Entry> = entries
            .iter()
            .enumerate()
            .filter(|&(at, _)| at != index)
            .map(|(_, entry)| *entry)
            .collect();
        let expected = (
            (!WIDE[index].default).then_some(ErrorKind::MissingField),
            tracked_before(&present, (present.len(), 0)),
        );
        for chunk in CHUNKS {
            assert_eq!(
                outcome::<Wide>(Profile::Default, &map(&present, None, b""), chunk),
                expected,
                "without {index} in chunks of {chunk}"
            );
        }
    }
}

#[derive(crate::Decode)]
struct Spoken<'a> {
    a: Tracked,
    b: Chunks<'a, str>,
    c: Tracked,
}

#[test]
fn a_chunked_string_drops_what_came_before_it_or_with_it() {
    let cases: [(&[u8], Outcome); 5] = [
        (b"\xa3\x61a\x01\x61b\x7f\x61x\x60\xff\x61c\x02", (None, 2)),
        (
            b"\xa3\x61a\x01\x61b\x7f\x61x\x41y\xff\x61c\x02",
            (Some(ErrorKind::NotWellFormed), 1),
        ),
        (
            b"\xa3\x61a\x01\x61b\x7f\x61x\x60\xff\x61c\xf5",
            (Some(ErrorKind::TypeMismatch), 1),
        ),
        (
            b"\xa3\x61a\x01\x61b\x7f\x61x",
            (Some(ErrorKind::NeedContiguous), 1),
        ),
        (b"\xa3\x61a\x01\x61b\x7f\x61x\x60\xff", (None, 1)),
    ];
    for (bytes, expected) in cases {
        assert_eq!(
            outcome::<Spoken>(Profile::Default, bytes, usize::MAX),
            expected,
            "{bytes:x?}"
        );
    }
}

/// An `other` field's captured entries borrow and need no drop; the fields around them still drop
/// exactly as decoded, whichever way the decode stops (R-128).
#[cfg(feature = "heapless")]
mod captured {
    use super::{Profile, Tracked, outcome};
    use crate::engine::RawEntry;
    use crate::error::ErrorKind;

    #[derive(crate::Decode)]
    struct Holding<'a> {
        a: Tracked,
        #[cbor(other)]
        _rest: heapless::Vec<RawEntry<'a>, 1>,
        g: Tracked,
    }

    #[test]
    fn a_decode_that_stops_around_captured_entries_drops_what_came_before() {
        let cases: [(&[u8], usize, Option<ErrorKind>); 5] = [
            (b"\xa3\x61a\x01\x61z\x02\x61g\x03", usize::MAX, None),
            (
                b"\xa3\x61a\x01\x61z\x02\x61g\xf5",
                usize::MAX,
                Some(ErrorKind::TypeMismatch),
            ),
            (
                b"\xa3\x61a\x01\x61y\x00\x61z\x00",
                usize::MAX,
                Some(ErrorKind::LimitExceeded),
            ),
            (
                b"\xa2\x61a\x01\x61z\x02",
                usize::MAX,
                Some(ErrorKind::MissingField),
            ),
            (
                b"\xa3\x61a\x01\x61z\x02\x61g\x03",
                1,
                Some(ErrorKind::NeedContiguous),
            ),
        ];
        for (bytes, chunk, error) in cases {
            assert_eq!(
                outcome::<Holding>(Profile::Default, bytes, chunk),
                (error, if error.is_none() { 2 } else { 1 }),
                "{bytes:x?} in chunks of {chunk}"
            );
        }
    }
}
