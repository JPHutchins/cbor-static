//! Internally (`#[serde(tag = "t")]`) and adjacently (`#[serde(tag = "t", content = "c")]`) tagged
//! enums (D-10). The variant's name may follow any other entry, so the decoder first scans the
//! map's rest in the current chunk for it: these enums need contiguous input (D-16). The layout
//! holds the variant names, then the tag key, then the content key.

use core::mem::MaybeUninit;
use core::ops::Range;

use super::enums::{assembled, child_done, drop};
use super::map::MAP;
use super::option::{done, holds_nothing, payload_only};
use super::{
    Begin, DecOps, Entry, Flow, State, Target, at, desc, end, entries, expect, next_entry, open,
    start_name, start_skip, top, unnamed,
};
use crate::engine::input::Input;
use crate::engine::skip::item_len;
use crate::error::ErrorKind;
use crate::schema::{DecKind, Layout};
use crate::stack::{Frame, INDEFINITE, Phase};
use crate::wire::{Header, Major, length};

pub static INTERNAL: DecOps = DecOps {
    value: Begin::Header(internal),
    entry: payload_only,
    named: unnamed,
    child_done: assembled,
    close: done,
    drop: holds_nothing,
};

pub static ADJACENT: DecOps = DecOps {
    value: Begin::Header(adjacent),
    entry: adjacent_entry,
    named: adjacent_named,
    child_done,
    close: adjacent_close,
    drop,
};

/// An internally tagged unit variant's payload: a map whose entries are all skipped.
static NOTHING: DecKind = DecKind::Struct {
    ops: &MAP,
    layout: &Layout {
        names: &[],
        fields: &[],
    },
    drop: None,
    kinds: &[],
};

/// Decodes the whole map as the named variant's payload struct, whose decoder skips the tag
/// entry like any key it does not know.
fn internal(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Enum {
        layout, variants, ..
    } = state.target.kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    if header.major != Major::Map {
        return Err(ErrorKind::TypeMismatch);
    }
    let count = entries(header)?;
    let variant = tagged(layout, variants.len(), count, input.rest())?;
    open(state, frames, Phase::EnumPayload, 0)?;
    top(state.depth, frames)?.field = variant;
    state.target = Target {
        kind: match variants
            .get(usize::from(variant))
            .ok_or(ErrorKind::Unsupported)?
            .payload
        {
            None => &NOTHING,
            Some(kind @ DecKind::Struct { .. }) => kind,
            Some(_) => return Err(ErrorKind::Unsupported),
        },
        dst: at(state.target.dst, layout, variant),
    };
    open(state, frames, Phase::Key, count)
}

fn adjacent(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
    header: Header,
) -> Result<Flow, ErrorKind> {
    let DecKind::Enum {
        layout, variants, ..
    } = state.target.kind
    else {
        return Err(ErrorKind::Unsupported);
    };
    if header.major != Major::Map {
        return Err(ErrorKind::TypeMismatch);
    }
    let count = entries(header)?;
    let variant = tagged(layout, variants.len(), count, input.rest())?;
    open(state, frames, Phase::EnumName, count)?;
    top(state.depth, frames)?.field = variant;
    Ok(Flow::Continue)
}

fn adjacent_entry(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    match next_entry(state, frames, input)? {
        Entry::Wait => Ok(Flow::NeedInput),
        Entry::End => end(state, frames),
        Entry::Item(header) if header.major == Major::Text && !header.indefinite() => {
            Ok(start_name(
                state,
                desc(top(state.depth, frames)?)
                    .layout()
                    .ok_or(ErrorKind::Unsupported)?,
                header.arg,
            ))
        }
        Entry::Item(header) => start_skip(state, 2, Some(header)),
    }
}

/// The content key starts the named variant's payload; the tag, already read, and any other key
/// are skipped.
fn adjacent_named(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    matched: Option<u8>,
) -> Result<Flow, ErrorKind> {
    let frame = top(state.depth, frames)?;
    let DecKind::Enum {
        layout, variants, ..
    } = desc(frame)
    else {
        return Err(ErrorKind::Unsupported);
    };
    match (matched.map(usize::from), frame.phase) {
        (Some(content), Phase::EnumName) if content == variants.len() + 1 => {
            let kind = variants
                .get(usize::from(frame.field))
                .and_then(|variant| variant.payload)
                .ok_or(ErrorKind::TypeMismatch)?;
            frame.phase = Phase::EnumPayload;
            Ok(expect(state, kind, at(frame.base, layout, frame.field)))
        }
        (Some(content), _) if content == variants.len() + 1 => Err(ErrorKind::DuplicateKey),
        _ => start_skip(state, 1, None),
    }
}

/// A unit variant has no content key; any other must have had one.
fn adjacent_close(frame: &mut Frame) -> Result<(), ErrorKind> {
    let DecKind::Enum { variants, .. } = desc(frame) else {
        return Err(ErrorKind::Unsupported);
    };
    match (
        frame.phase,
        variants
            .get(usize::from(frame.field))
            .map(|v| (v.payload, v.assemble)),
    ) {
        (Phase::EnumDone, _) => Ok(()),
        (Phase::EnumName, Some((None, assemble))) => {
            unsafe { assemble(frame.base) };
            frame.phase = Phase::EnumDone;
            Ok(())
        }
        _ => Err(ErrorKind::MissingField),
    }
}

/// The variant the map in `bytes`, of `count` entries, names under its tag key: the layout entry
/// after the `variants` names.
fn tagged(layout: &Layout, variants: usize, count: u64, bytes: &[u8]) -> Result<u8, ErrorKind> {
    let tag = variants..variants + 1;
    let mut at = 0;
    let mut left = count;
    loop {
        if left == 0 {
            return Err(ErrorKind::MissingField);
        }
        let (key, name) = text(bytes, at)?;
        match (left, key.is_break()) {
            (INDEFINITE, true) => return Err(ErrorKind::MissingField),
            (INDEFINITE, false) => {}
            _ => left -= 1,
        }
        let value = at + item_len(bytes.get(at..).unwrap_or(&[]))?;
        if name.is_some_and(|key| named(layout, tag.clone(), key).is_some()) {
            return text(bytes, value)?
                .1
                .ok_or(ErrorKind::TypeMismatch)
                .and_then(|name| {
                    named(layout, 0..variants, name).ok_or(ErrorKind::UnknownVariant)
                });
        }
        at = value + item_len(bytes.get(value..).unwrap_or(&[]))?;
    }
}

/// The header of the item at `at`, and its payload when it is a definite text.
fn text(bytes: &[u8], at: usize) -> Result<(Header, Option<&[u8]>), ErrorKind> {
    let mut input = Input::new(bytes.get(at..).unwrap_or(&[]));
    let header = input.take_head()?.ok_or(ErrorKind::NeedContiguous)?;
    match (header.major, header.indefinite()) {
        (Major::Text, false) => input
            .take_exact(length(header.arg)?)
            .map(|payload| (header, Some(payload)))
            .ok_or(ErrorKind::NeedContiguous),
        _ => Ok((header, None)),
    }
}

/// The entry among `entries` whose text name is `name`.
fn named(layout: &Layout, entries: Range<usize>, name: &[u8]) -> Option<u8> {
    entries
        .filter_map(|index| Some((u8::try_from(index).ok()?, layout.fields.get(index)?)))
        .find(|(_, place)| {
            layout
                .names
                .get(usize::from(place.name_at)..)
                .and_then(|names| names.get(..usize::from(place.name_len)))
                == Some(name)
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::tagged;
    use crate::error::ErrorKind;
    use crate::keys::{Key, field, names, names_len};
    use crate::schema::Layout;

    const KEYS: &[Key] = &[Key::Text("A"), Key::Text("B"), Key::Text("t")];
    const NAMES: [u8; names_len(KEYS)] = names(KEYS);
    const LAYOUT: Layout = Layout {
        names: &NAMES,
        fields: &[
            field(KEYS, 0, 0, false),
            field(KEYS, 1, 0, false),
            field(KEYS, 2, 0, false),
        ],
    };

    /// `bytes` is a map's entries after its header, which says `count` (or indefinite).
    fn scan(count: u64, bytes: &[u8]) -> Result<u8, ErrorKind> {
        tagged(&LAYOUT, 2, count, bytes)
    }

    #[test]
    fn the_tag_is_found_after_any_entries_it_skips() {
        assert_eq!(scan(1, b"\x61t\x61B"), Ok(1));
        assert_eq!(
            scan(
                3,
                b"\x01\x9f\xa1\x61x\x5f\x41a\xff\xff\x61y\x82\x01\x02\x61t\x61A"
            ),
            Ok(0)
        );
        assert_eq!(scan(u64::MAX, b"\x61x\x01\x61t\x61B\xff"), Ok(1));
    }

    #[test]
    fn a_missing_cut_or_mistyped_tag_is_an_error() {
        let cases: [(u64, &[u8], ErrorKind); 6] = [
            (1, b"\x61x\x01", ErrorKind::MissingField),
            (u64::MAX, b"\x61x\x01\xff", ErrorKind::MissingField),
            (2, b"\x61x\x01", ErrorKind::NeedContiguous),
            (1, b"\x61t\x61", ErrorKind::NeedContiguous),
            (1, b"\x61t\x01", ErrorKind::TypeMismatch),
            (1, b"\x61t\x61C", ErrorKind::UnknownVariant),
        ];
        for (count, bytes, error) in cases {
            assert_eq!(scan(count, bytes), Err(error), "{bytes:x?}");
        }
    }
}
