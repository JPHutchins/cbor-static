//! Structs as maps keyed by text, by integer (D-09), or both, whose unknown entries are skipped or,
//! with `deny_unknown_fields`, rejected. Each combination is its own ops table, so a firmware links
//! only the key matching it uses: integer keys that deny unknown entries link no skip, no key
//! matcher and no UTF-8 validation.

use core::mem::MaybeUninit;

use super::{
    After, Begin, DecOps, Entry, Flow, Scratch, State, at, desc, end, entries, expect, name,
    next_entry, open, skip_unknown, start_name, top,
};
use crate::engine::bit;
use crate::engine::input::Input;
use crate::engine::text::KeyMatch;
use crate::error::ErrorKind;
use crate::schema::{DecKind, Layout};
use crate::stack::{Frame, Phase};
use crate::wire::{Header, Major, parse};

pub static MAP: DecOps = ops::<true, false, false>();
pub static MAP_INT: DecOps = ops::<false, true, false>();
pub static MAP_MIXED: DecOps = ops::<true, true, false>();
pub static MAP_DENY: DecOps = ops::<true, false, true>();
pub static MAP_INT_DENY: DecOps = ops::<false, true, true>();
pub static MAP_MIXED_DENY: DecOps = ops::<true, true, true>();

const fn ops<const TEXT: bool, const INT: bool, const DENY: bool>() -> DecOps {
    DecOps {
        value: Begin::Header(value),
        entry: entry::<TEXT, INT, DENY>,
        named: named::<DENY>,
        child_done,
        close,
        drop,
    }
}

fn value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    _: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    match header.major {
        Major::Map => open(state, frames, Phase::Key, entries(header)?),
        _ => Err(ErrorKind::TypeMismatch),
    }
}

fn entry<const TEXT: bool, const INT: bool, const DENY: bool>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    match next_entry(state, frames, input)? {
        Entry::Wait => Ok(Flow::NeedInput),
        Entry::End => end(state, frames),
        Entry::Item(header) => match (header.major, header.indefinite()) {
            (Major::Text, false) if TEXT => Ok(start_text_key::<INT>(
                state,
                desc(top(state.depth, frames)?)
                    .layout()
                    .ok_or(ErrorKind::Unsupported)?,
                header.arg,
            )),
            (Major::Text, true) if TEXT => Err(ErrorKind::Unsupported),
            (Major::Uint | Major::Nint, _) if INT => match item_index(
                desc(top(state.depth, frames)?)
                    .layout()
                    .ok_or(ErrorKind::Unsupported)?,
                header,
            ) {
                Some(field) => select(state, frames, field),
                None => unknown::<DENY>(state, frames, 1, None),
            },
            _ => unknown::<DENY>(state, frames, 2, Some(header)),
        },
    }
}

fn named<const DENY: bool>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    matched: Option<u8>,
) -> Result<Flow, ErrorKind> {
    match matched {
        None => unknown::<DENY>(state, frames, 1, None),
        Some(field) => select(state, frames, field),
    }
}

/// Decodes the value of `field` next, unless the map already had it.
fn select(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    field: u8,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    if frame.seen & bit(field) != 0 {
        return Err(ErrorKind::DuplicateKey);
    }
    if let Some(strict) = state.strict {
        (strict.order)(layout, frame, field)?;
    }
    frame.field = field;
    Ok(expect(
        state,
        kinds
            .get(usize::from(field))
            .ok_or(ErrorKind::Unsupported)?,
        at(frame.base, layout, field),
    ))
}

/// Skips the rest of an entry no field claims, `items` items of which remain, or rejects it unless
/// it is an internally tagged enum's tag entry.
fn unknown<const DENY: bool>(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    items: u64,
    header: Option<Header>,
) -> Result<Flow, ErrorKind> {
    use super::tagged::admit;

    if DENY {
        admit(state, frames, header)
    } else {
        skip_unknown(state, frames, items, header)
    }
}

/// Matches a text key; in a mixed map, only against the text-keyed entries.
fn start_text_key<const INT: bool>(state: &mut State, layout: &'static Layout, len: u64) -> Flow {
    if INT {
        state.scratch.write(Scratch {
            key: KeyMatch::start(layout, len).excluding(heads(layout)),
        });
        state.step = name;
        Flow::Continue
    } else {
        start_name(state, layout, len)
    }
}

/// The entries identified by a pre-encoded head (an integer key or a tag) rather than a text.
fn heads(layout: &Layout) -> u64 {
    (0..layout.fields.len())
        .filter_map(|index| u8::try_from(index).ok())
        .filter(|&index| head(layout, index).is_some())
        .fold(0, |mask, index| mask | bit(index))
}

/// The entry whose pre-encoded head has `header`'s major type and argument, in any encoding.
pub(super) fn item_index(layout: &Layout, header: Header) -> Option<u8> {
    (0..layout.fields.len())
        .filter_map(|index| u8::try_from(index).ok())
        .find(|&index| {
            head(layout, index)
                .is_some_and(|item| (item.major, item.arg) == (header.major, header.arg))
        })
}

/// Entry `index`'s identifier when it is a head: integer and tag heads are under 24 bytes and
/// begin one byte before `name_at`, where a text identifier has its own initial byte instead.
fn head(layout: &Layout, index: u8) -> Option<Header> {
    let place = layout.fields.get(usize::from(index))?;
    let start = usize::from(place.name_at).checked_sub(1)?;
    (place.name_len < 24)
        .then(|| {
            layout
                .names
                .get(start..usize::from(place.name_at) + usize::from(place.name_len))
        })
        .flatten()
        .and_then(|item| parse(item).ok())
        .filter(|item| item.major != Major::Text)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "an enum's variant lookup can fail"
)]
fn child_done(frame: &mut Frame) -> Result<After, ErrorKind> {
    frame.seen |= bit(frame.field);
    Ok(After::Entry)
}

/// Writes the default of every field the map did not contain.
pub(super) fn close(frame: &mut Frame) -> Result<(), ErrorKind> {
    let DecKind::Struct { layout, kinds, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    (0..kinds.len())
        .filter_map(|f| u8::try_from(f).ok())
        .try_for_each(|field| {
            if frame.seen & bit(field) != 0 {
                return Ok(());
            }
            frame.field = field;
            let write = kinds
                .get(usize::from(field))
                .and_then(|kind| kind.default_fn())
                .ok_or(ErrorKind::MissingField)?;
            unsafe { write(at(frame.base, layout, field)) };
            frame.seen |= bit(field);
            Ok(())
        })
}

/// Drops exactly the fields the map initialized.
#[inline(never)]
pub(super) fn drop(frame: &Frame) {
    if let DecKind::Struct { layout, kinds, .. } = desc(frame) {
        (0..kinds.len())
            .filter_map(|f| u8::try_from(f).ok())
            .filter(|&field| frame.seen & bit(field) != 0)
            .for_each(|field| {
                if let Some(drop) = kinds
                    .get(usize::from(field))
                    .and_then(|kind| kind.drop_fn())
                {
                    unsafe { drop(at(frame.base, layout, field)) };
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::{heads, item_index};
    use crate::keys::{Key, field, names, names_len};
    use crate::schema::Layout;
    use crate::wire::{header_len, parse};

    const KEYS: &[Key] = &[
        Key::Text("a"),
        Key::Int(97),
        Key::Int(-1000),
        Key::Tag(18),
        Key::Text("twenty-four bytes long!!"),
    ];
    const NAMES: [u8; names_len(KEYS)] = names(KEYS);
    const LAYOUT: Layout = Layout {
        names: &NAMES,
        fields: &[
            field(KEYS, 0, 0, false),
            field(KEYS, 1, 0, false),
            field(KEYS, 2, 0, false),
            field(KEYS, 3, 0, false),
            field(KEYS, 4, 0, false),
        ],
    };

    #[test]
    fn a_skipped_entry_is_never_a_head_whatever_precedes_it() {
        const SKIPPING: &[Key] = &[Key::Text("x1"), Key::Skipped, Key::Int(5)];
        const SKIPPING_NAMES: [u8; names_len(SKIPPING)] = names(SKIPPING);
        const SKIPPING_LAYOUT: Layout = Layout {
            names: &SKIPPING_NAMES,
            fields: &[
                field(SKIPPING, 0, 0, false),
                field(SKIPPING, 1, 0, true),
                field(SKIPPING, 2, 0, false),
            ],
        };
        assert_eq!(SKIPPING_NAMES, *b"\x62x1\x61\xff\x05");
        assert_eq!(heads(&SKIPPING_LAYOUT), 0b100);
        for bytes in [b"\x31".as_slice(), b"\x61", b"\x05"] {
            let header = parse(bytes).expect("a head");
            assert_eq!(
                item_index(&SKIPPING_LAYOUT, header),
                (bytes == b"\x05").then_some(2),
                "{bytes:x?}"
            );
        }
    }

    #[test]
    fn heads_are_the_integer_and_tag_entries() {
        assert_eq!(heads(&LAYOUT), 0b01110);
    }

    #[test]
    fn an_item_matches_the_entry_with_its_head_in_any_encoding() {
        let cases: [(&[u8], Option<u8>); 8] = [
            (b"\x18\x61", Some(1)),
            (b"\x19\x00\x61", Some(1)),
            (b"\x39\x03\xe7", Some(2)),
            (b"\xd2", Some(3)),
            (b"\xd8\x12", Some(3)),
            (b"\x61", None),
            (b"\x38\x61", None),
            (b"\x18\x18", None),
        ];
        for (bytes, expected) in cases {
            let header = header_len(bytes[0]).and_then(|_| parse(bytes));
            assert_eq!(
                header.map(|header| item_index(&LAYOUT, header)),
                Ok(expected),
                "{bytes:x?}"
            );
        }
    }
}
