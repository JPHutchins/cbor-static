#![forbid(unsafe_code)]
//! Entry identifiers pre-encoded at compile time into one blob per layout, so that a key known
//! only to const evaluation (`Label::Alg as i64`, D-09) costs the same as a literal.
//!
//! An entry's `name_at`/`name_len` follow one rule for every identifier the encoder writes: its
//! item spans `name_at - h .. name_at + name_len`, where `h` is 1, or 2 once `name_len` reaches
//! 24. A text item's payload is exactly `name_at .. name_at + name_len`.

use crate::engine::offset;
use crate::lead::Lead;
use crate::schema::{FieldLayout, Layout};
use crate::wire::{Head, Major};

/// An entry's identifier on the wire.
pub enum Key {
    Text(&'static str),
    Int(i64),
    Tag(u64),
    /// The first-item classes an untagged variant accepts, as [`Lead::to_le_bytes`]; never written.
    Lead(Lead),
    /// Positional: no identifier.
    None,
    /// A `skip`ped field's: one byte of text that is not UTF-8, which no key can match.
    Skipped,
}

/// The total length of `keys`' pre-encoded identifiers.
#[must_use]
pub const fn names_len(keys: &[Key]) -> usize {
    match keys.split_first() {
        None => 0,
        Some((key, rest)) => item_len(key) + names_len(rest),
    }
}

/// `keys`' identifiers, pre-encoded and concatenated; `N` is [`names_len`].
///
/// # Panics
/// At compile time, when `N` is not [`names_len`].
#[must_use]
pub const fn names<const N: usize>(keys: &[Key]) -> [u8; N] {
    assert!(N == names_len(keys), "cbor-static: names_len mismatch");
    written(keys, [0; N], 0)
}

/// The layout of entry `index` of `keys`, `offset` bytes into its parent.
///
/// # Panics
/// At compile time, when the blob outgrows `u16` or a name outgrows `u8`.
#[must_use]
pub const fn field(keys: &[Key], index: usize, at: usize, omit_if_absent: bool) -> FieldLayout {
    let start = names_len(keys.split_at(index).0) + prefix(&keys[index]);
    let len = name_len(&keys[index]);
    assert!(
        start <= u16::MAX as usize,
        "cbor-static: names exceed 64 KiB"
    );
    assert!(
        len <= u8::MAX as usize,
        "cbor-static: a name exceeds 255 bytes"
    );
    FieldLayout {
        offset: offset(at),
        #[expect(clippy::cast_possible_truncation, reason = "asserted above")]
        name_at: start as u16,
        #[expect(clippy::cast_possible_truncation, reason = "asserted above")]
        name_len: len as u8,
        omit_if_absent,
    }
}

/// Whether an entry of `layout` is named by the text `name`. An internally tagged enum's tag shares
/// its payload's map, so no payload field may be keyed like it (D-10).
#[must_use]
pub const fn names_text(layout: Option<&Layout>, name: &str) -> bool {
    match layout {
        None => false,
        Some(layout) => names_from(
            layout,
            Head::new(Major::Text, name.len() as u64).as_bytes(),
            name.as_bytes(),
            0,
        ),
    }
}

const fn names_from(layout: &Layout, head: &[u8], name: &[u8], index: usize) -> bool {
    match layout.fields.split_at(index).1.split_first() {
        None => false,
        Some((place, _)) => {
            (place.name_len as usize == name.len()
                && place.name_at as usize >= head.len()
                && holds(layout.names, place.name_at as usize - head.len(), head)
                && holds(layout.names, place.name_at as usize, name))
                || names_from(layout, head, name, index + 1)
        }
    }
}

/// Whether `blob` holds `bytes` from `at`.
const fn holds(blob: &[u8], at: usize, bytes: &[u8]) -> bool {
    match bytes.split_first() {
        None => true,
        Some((&byte, rest)) => at < blob.len() && blob[at] == byte && holds(blob, at + 1, rest),
    }
}

const fn head(key: &Key) -> Option<Head> {
    match *key {
        Key::Text(text) => Some(Head::new(Major::Text, text.len() as u64)),
        Key::Int(int) if int < 0 => Some(Head::new(Major::Nint, !int.cast_unsigned())),
        Key::Int(int) => Some(Head::new(Major::Uint, int.cast_unsigned())),
        Key::Tag(tag) => Some(Head::new(Major::Tag, tag)),
        Key::Skipped => Some(Head::new(Major::Text, 1)),
        Key::Lead(_) | Key::None => None,
    }
}

const fn payload(key: &Key) -> &[u8] {
    match key {
        Key::Text(text) => text.as_bytes(),
        Key::Int(_) | Key::Tag(_) | Key::None => &[],
        Key::Lead(_) => &[0; Lead::ENCODED_LEN],
        Key::Skipped => &[0xff],
    }
}

const fn head_len(key: &Key) -> usize {
    match head(key) {
        Some(head) => head.as_bytes().len(),
        None => 0,
    }
}

const fn item_len(key: &Key) -> usize {
    head_len(key) + payload(key).len()
}

/// Bytes of the item before `name_at`.
const fn prefix(key: &Key) -> usize {
    match key {
        Key::Text(_) | Key::Skipped => head_len(key),
        Key::Int(_) | Key::Tag(_) => 1,
        Key::Lead(_) | Key::None => 0,
    }
}

const fn name_len(key: &Key) -> usize {
    item_len(key) - prefix(key)
}

const fn written<const N: usize>(keys: &[Key], blob: [u8; N], at: usize) -> [u8; N] {
    match keys.split_first() {
        None => blob,
        Some((key, rest)) => written(rest, item(key, blob, at), at + item_len(key)),
    }
}

const fn item<const N: usize>(key: &Key, blob: [u8; N], at: usize) -> [u8; N] {
    match (key, head(key)) {
        (Key::Lead(lead), _) => copied(blob, at, &lead.to_le_bytes()),
        (_, Some(head)) => copied(
            copied(blob, at, head.as_bytes()),
            at + head_len(key),
            payload(key),
        ),
        (_, None) => blob,
    }
}

const fn copied<const N: usize>(blob: [u8; N], at: usize, bytes: &[u8]) -> [u8; N] {
    match bytes.split_first() {
        None => blob,
        Some((&byte, rest)) => {
            let mut next = blob;
            next[at] = byte;
            copied(next, at + 1, rest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, field, names, names_len};
    use crate::lead::Lead;
    use crate::schema::Layout;

    const KEYS: &[Key] = &[
        Key::Text("id"),
        Key::Int(-1),
        Key::Int(1000),
        Key::Tag(18),
        Key::Text("a twenty-four byte name!"),
        Key::Lead(Lead::from_le_bytes([
            0x23, 0x01, 1, 18, 0, 0, 0, 0, 0, 0, 0,
        ])),
        Key::None,
    ];
    const NAMES: [u8; names_len(KEYS)] = names(KEYS);
    const LAYOUT: Layout = Layout {
        names: &NAMES,
        fields: &[
            field(KEYS, 0, 0, false),
            field(KEYS, 1, 4, true),
            field(KEYS, 2, 8, false),
            field(KEYS, 3, 12, false),
            field(KEYS, 4, 16, false),
        ],
    };

    #[test]
    fn every_identifier_is_pre_encoded_in_order() {
        assert_eq!(
            NAMES.as_slice(),
            [
                b"\x62id\x20\x19\x03\xe8\xd2\x78\x18".as_slice(),
                b"a twenty-four byte name!",
                b"\x23\x01\x01\x12\0\0\0\0\0\0\0",
            ]
            .concat()
        );
    }

    #[test]
    fn only_a_text_entry_names_a_text() {
        assert!(super::names_text(Some(&LAYOUT), "id"));
        assert!(super::names_text(Some(&LAYOUT), "a twenty-four byte name!"));
        assert!(!super::names_text(Some(&LAYOUT), "i"));
        assert!(!super::names_text(Some(&LAYOUT), "\x19"));
        assert!(!super::names_text(None, "id"));
    }

    #[test]
    fn the_encoder_recovers_each_item_and_the_matcher_each_text_payload() {
        let items: [&[u8]; 5] = [b"\x62id", b"\x20", b"\x19\x03\xe8", b"\xd2", &NAMES[8..34]];
        for (index, item) in (0..).zip(items) {
            assert_eq!(LAYOUT.name_item(index), item);
        }
        let text = &LAYOUT.fields[4];
        assert_eq!(
            &NAMES[usize::from(text.name_at)..][..usize::from(text.name_len)],
            b"a twenty-four byte name!"
        );
        assert_eq!(
            (LAYOUT.fields[1].offset, LAYOUT.fields[1].omit_if_absent),
            (4, true)
        );
    }
}
