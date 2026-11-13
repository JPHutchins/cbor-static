//! Internally (`#[serde(tag = "t")]`) and adjacently (`#[serde(tag = "t", content = "c")]`) tagged
//! enums (D-10). The variant's name may follow any other entry, so the decoder first scans the
//! whole map in the current chunk for it, and for a second tag key: these enums need contiguous
//! input (D-16). The layout holds the variant names, then the tag key, then the content key.

use core::mem::MaybeUninit;
use core::ops::Range;

use super::enums::{assembled, child_done, drop};
use super::map::MAP;
use super::option::{done, holds_nothing, payload_only};
use super::{
    Begin, DecOps, Entry, Flow, State, Target, at, desc, end, entries, expect, next_entry, open,
    ops, skip_unknown, start_name, start_skip, top, unnamed,
};
use crate::engine::StrictOps;
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

/// Decodes the whole map as the named variant's payload struct, a map-form struct by a check at
/// compile time, whose decoder skips the tag entry like any key it does not know, and claims it
/// where it would reject one ([`claim_tag`]).
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
    let (count, variant) = tagged_map(state.strict, layout, variants.len(), input, header)?;
    open(state, frames, Phase::EnumPayload, 0)?;
    top(state.depth, frames)?.field = variant;
    state.target = Target {
        kind: variants
            .get(usize::from(variant))
            .ok_or(ErrorKind::Unsupported)?
            .payload
            .unwrap_or(&NOTHING),
        dst: at(state.target.dst, layout, variant),
    };
    open(state, frames, Phase::TagAhead, count)
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
    let (count, variant) = tagged_map(state.strict, layout, variants.len(), input, header)?;
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
        Entry::Item(header) => skip_unknown(state, frames, 2, Some(header)),
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
        (Some(tag), _) if tag == variants.len() => start_skip(state, 1, None),
        _ => skip_unknown(state, frames, 1, None),
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

/// The entries of the map `header` opens, and the variant its tag names; under `Strict`, the map's
/// keys, the tag key included, must rise.
fn tagged_map(
    strict: Option<&'static StrictOps>,
    layout: &Layout,
    variants: usize,
    input: &Input,
    header: Header,
) -> Result<(u64, u8), ErrorKind> {
    if header.major != Major::Map {
        return Err(ErrorKind::TypeMismatch);
    }
    let count = entries(header)?;
    if let Some(strict) = strict {
        (strict.sorted)(input.rest(), count)?;
    }
    Ok((count, tagged(layout, variants, count, input.rest())?))
}

/// The variant the map in `bytes`, of `count` entries, names under its tag key: the layout entry
/// after the `variants` names. The whole map is walked, so a second tag key is a duplicate.
fn tagged(layout: &Layout, variants: usize, count: u64, bytes: &[u8]) -> Result<u8, ErrorKind> {
    let tag = variants..variants + 1;
    let mut at = 0;
    let mut left = count;
    let mut found = None;
    loop {
        if left == 0 {
            return found.ok_or(ErrorKind::MissingField);
        }
        let (key, name) = text(bytes, at)?;
        match (left, key.is_break()) {
            (INDEFINITE, true) => return found.ok_or(ErrorKind::MissingField),
            (INDEFINITE, false) => {}
            _ => left -= 1,
        }
        let value = at + item_len(bytes.get(at..).unwrap_or(&[]))?;
        if name.is_some_and(|key| named(layout, tag.clone(), key).is_some()) {
            if found.is_some() {
                return Err(ErrorKind::DuplicateKey);
            }
            found = Some(
                text(bytes, value)?
                    .1
                    .ok_or(ErrorKind::TypeMismatch)
                    .and_then(|name| {
                        named(layout, 0..variants, name).ok_or(ErrorKind::UnknownVariant)
                    })?,
            );
        }
        at = value + item_len(bytes.get(value..).unwrap_or(&[]))?;
    }
}

/// Claims an entry no field of the top frame's map knows as the internal tag entry, while the map
/// still has it ahead: the one unknown entry an internally tagged payload admits, under `Strict` or
/// `deny_unknown_fields`. The scan already found exactly one tag entry, and any other unknown entry
/// is an error either way, so claiming the first unknown one decides the same.
pub(crate) fn claim_tag(frames: &mut [MaybeUninit<Frame>], depth: u8) -> Result<(), ErrorKind> {
    let frame = top(depth, frames)?;
    match frame.phase {
        Phase::TagAhead => {
            frame.phase = Phase::Key;
            Ok(())
        }
        _ => Err(ErrorKind::UnknownField),
    }
}

/// Under `deny_unknown_fields`, an entry no field claims is an error unless [`claim_tag`] claims it.
/// Only a text key can be the tag: one the key matcher took whole (`header` is `None`), or, in a map
/// of integer keys, one whose `header` was read, which a tag key's 255-byte limit keeps short.
/// [`tag_value`] takes the rest without the skip machinery, so a map that denies unknown entries
/// still links none of it.
pub(super) fn admit(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    header: Option<Header>,
) -> Result<Flow, ErrorKind> {
    let key_left = match header {
        None => 0,
        Some(key) if key.major == Major::Text && !key.indefinite() => {
            u8::try_from(key.arg).map_err(|_| ErrorKind::UnknownField)?
        }
        Some(_) => return Err(ErrorKind::UnknownField),
    };
    claim_tag(frames, state.depth)?;
    top(state.depth, frames)?.field = key_left;
    state.step = tag_value;
    Ok(Flow::Continue)
}

/// The rest of a claimed tag entry from contiguous input: what is left of its key (frame `field`
/// bytes), then its value, which as the tag's is a definite text string. Any other value marks a
/// stray entry, not the tag.
fn tag_value(
    state: &mut State,
    frames: &mut [MaybeUninit<Frame>],
    input: &mut Input,
) -> Result<Flow, ErrorKind> {
    input
        .take_exact(usize::from(top(state.depth, frames)?.field))
        .ok_or(ErrorKind::NeedContiguous)?;
    match state.carry.header(input, state.strict)? {
        Some(value) if value.major == Major::Text && !value.indefinite() => {
            input
                .take_exact(length(value.arg)?)
                .ok_or(ErrorKind::NeedContiguous)?;
            state.step = ops(desc(top(state.depth, frames)?))?.entry;
            Ok(Flow::Continue)
        }
        Some(_) => Err(ErrorKind::UnknownField),
        None => Err(ErrorKind::NeedContiguous),
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
    fn a_missing_cut_mistyped_or_repeated_tag_is_an_error() {
        let cases: [(u64, &[u8], ErrorKind); 10] = [
            (1, b"\x61x\x01", ErrorKind::MissingField),
            (u64::MAX, b"\x61x\x01\xff", ErrorKind::MissingField),
            (2, b"\x61x\x01", ErrorKind::NeedContiguous),
            (1, b"\x61t\x61", ErrorKind::NeedContiguous),
            (2, b"\x61t\x61A", ErrorKind::NeedContiguous),
            (1, b"\x61t\x01", ErrorKind::TypeMismatch),
            (1, b"\x61t\x61C", ErrorKind::UnknownVariant),
            (2, b"\x61t\x61A\x61t\x61B", ErrorKind::DuplicateKey),
            (3, b"\x61t\x61A\x61x\x01\x61t\x61A", ErrorKind::DuplicateKey),
            (
                u64::MAX,
                b"\x61t\x61B\x61t\x61B\xff",
                ErrorKind::DuplicateKey,
            ),
        ];
        for (count, bytes, error) in cases {
            assert_eq!(scan(count, bytes), Err(error), "{bytes:x?}");
        }
    }
}
