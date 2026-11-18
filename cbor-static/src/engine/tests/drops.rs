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

fn every_item(table: &[Entry]) -> impl Iterator<Item = (usize, usize)> {
    table
        .iter()
        .enumerate()
        .flat_map(|(index, entry)| (0..entry.items.len()).map(move |item| (index, item)))
}

fn all() -> Vec<&'static Entry> {
    WIDE.iter().collect()
}

#[test]
fn a_mismatch_at_any_item_drops_what_came_before() {
    mismatch_anywhere::<Wide>(&WIDE);
}

#[test]
fn input_cut_anywhere_drops_what_was_complete() {
    cut_anywhere::<Wide>(&WIDE);
}

/// A `T` whose map's entries are `table`, with `false` in place of any one item, under either
/// profile, drops the `Tracked` items before it.
fn mismatch_anywhere<T: for<'de> Decode<'de>>(table: &[Entry]) {
    let entries: Vec<&Entry> = table.iter().collect();
    for profile in [Profile::Default, Profile::Strict] {
        for at in every_item(table) {
            let bytes = map(&entries, Some(at), b"\xf5");
            for chunk in CHUNKS {
                assert_eq!(
                    outcome::<T>(profile, &bytes, chunk),
                    (Some(ErrorKind::TypeMismatch), tracked_before(&entries, at)),
                    "{at:?} in chunks of {chunk}"
                );
            }
        }
    }
}

/// A `T` whose map's entries are `table`, cut at any byte, drops the `Tracked` items complete.
fn cut_anywhere<T: for<'de> Decode<'de>>(table: &[Entry]) {
    let bytes = map(&table.iter().collect::<Vec<_>>(), None, b"");
    let ends: Vec<(usize, bool)> = table
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
                outcome::<T>(Profile::Default, &bytes[..cut], chunk),
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

/// Fields that allocate drop exactly what was decoded, a partial element's allocation included,
/// which Miri checks frees every allocation (R-114, #5 Q20).
#[cfg(feature = "alloc")]
mod allocated {
    use std::boxed::Box;
    use std::collections::BTreeMap;
    use std::string::String;
    use std::vec::Vec;

    use super::{Entry, Profile, Tracked, cut_anywhere, map, mismatch_anywhere, outcome};
    use crate::error::ErrorKind;

    #[derive(crate::Decode)]
    struct Holding {
        a: Tracked,
        b: Box<(Tracked, Tracked)>,
        g: Tracked,
        #[expect(
            clippy::zero_sized_map_values,
            reason = "each Tracked value counts its drop"
        )]
        m: BTreeMap<u8, Tracked>,
        s: String,
        v: Vec<Tracked>,
        w: Vec<String>,
    }

    /// `Holding`'s entries, in its keys' bytewise order.
    const HOLDING: [Entry; 7] = [
        Entry {
            key: b"\x61a",
            items: &[(b"\x01", true)],
            default: false,
        },
        Entry {
            key: b"\x61b",
            items: &[(b"\x82", false), (b"\x02", true), (b"\x03", true)],
            default: false,
        },
        Entry {
            key: b"\x61g",
            items: &[(b"\x04", true)],
            default: false,
        },
        Entry {
            key: b"\x61m",
            items: &[
                (b"\xa2", false),
                (b"\x01", false),
                (b"\x09", true),
                (b"\x02", false),
                (b"\x0a", true),
            ],
            default: false,
        },
        Entry {
            key: b"\x61s",
            items: &[(b"\x62xy", false)],
            default: false,
        },
        Entry {
            key: b"\x61v",
            items: &[(b"\x82", false), (b"\x05", true), (b"\x06", true)],
            default: false,
        },
        Entry {
            key: b"\x61w",
            items: &[(b"\x82", false), (b"\x61p", false), (b"\x62qr", false)],
            default: false,
        },
    ];

    #[test]
    fn a_mismatch_at_any_item_drops_what_came_before() {
        mismatch_anywhere::<Holding>(&HOLDING);
    }

    #[test]
    fn input_cut_anywhere_drops_what_was_complete() {
        cut_anywhere::<Holding>(&HOLDING);
    }

    /// Item `at`, read as `with`, stops a decode under `profile` with `error`, after `drops`.
    struct Partial {
        profile: Profile,
        at: (usize, usize),
        with: &'static [u8],
        error: ErrorKind,
        drops: usize,
    }

    const PARTIAL: [Partial; 7] = [
        Partial {
            profile: Profile::Default,
            at: (1, 2),
            with: b"\x80",
            error: ErrorKind::TypeMismatch,
            drops: 2,
        },
        Partial {
            profile: Profile::Default,
            at: (3, 3),
            with: b"\x01",
            error: ErrorKind::DuplicateKey,
            drops: 6,
        },
        Partial {
            profile: Profile::Strict,
            at: (3, 3),
            with: b"\x01",
            error: ErrorKind::NotDeterministic,
            drops: 5,
        },
        Partial {
            profile: Profile::Default,
            at: (4, 0),
            with: b"\x7f\x61x\x61\xff\xff",
            error: ErrorKind::InvalidUtf8,
            drops: 6,
        },
        Partial {
            profile: Profile::Default,
            at: (6, 2),
            with: b"\x7f\x61q\x41r\xff",
            error: ErrorKind::NotWellFormed,
            drops: 8,
        },
        Partial {
            profile: Profile::Default,
            at: (6, 2),
            with: b"\x62q\xff",
            error: ErrorKind::InvalidUtf8,
            drops: 8,
        },
        Partial {
            profile: Profile::Strict,
            at: (6, 2),
            with: b"\x7f\x61q\xff",
            error: ErrorKind::NotDeterministic,
            drops: 8,
        },
    ];

    #[test]
    fn a_failure_inside_a_partial_element_drops_what_came_before() {
        let entries: Vec<&Entry> = HOLDING.iter().collect();
        for partial in PARTIAL {
            for chunk in super::CHUNKS {
                assert_eq!(
                    outcome::<Holding>(
                        partial.profile,
                        &map(&entries, Some(partial.at), partial.with),
                        chunk
                    ),
                    (Some(partial.error), partial.drops),
                    "{at:?} as {with:x?} in chunks of {chunk}",
                    at = partial.at,
                    with = partial.with
                );
            }
        }
    }
}
