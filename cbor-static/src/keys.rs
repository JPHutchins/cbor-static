#![forbid(unsafe_code)]
//! Entry identifiers pre-encoded at compile time into one blob per layout, so that a key known
//! only to const evaluation (`Label::Alg as i64`, D-09) costs the same as a literal.
//!
//! An entry's `name_at`/`name_len` follow one rule for every identifier the encoder writes: its
//! item spans `name_at - h .. name_at + name_len`, where `h` is 1, or 2 once `name_len` reaches
//! 24. A text item's payload is exactly `name_at .. name_at + name_len`.

use crate::engine::{
    DECODE_MAP, DECODE_MAP_DENY, DECODE_MAP_INT, DECODE_MAP_INT_DENY, DECODE_MAP_INT_OTHER,
    DECODE_MAP_MIXED, DECODE_MAP_MIXED_DENY, DECODE_MAP_MIXED_OTHER, DECODE_MAP_OTHER, DecOps,
    ENCODE_MAP, ENCODE_MAP_OTHER, EncOps, filled, get, offset, set,
};
use crate::lead::Lead;
use crate::schema::{FieldLayout, Layout, Storage, head_before};
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
    /// An `other` field's, another such byte: the entries no key matches go to it (R-128).
    Other,
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

/// The order of `fields`' identifiers in `names` under deterministic encoding (D-11b), as indices
/// into `fields`: bytewise by their encodings (RFC 8949 §4.2.1), or with `length_first`, shorter
/// encodings first (§4.2.3). Entries that encode alike keep their order.
#[must_use]
pub const fn sorted<S: Storage<u8>>(names: &[u8], fields: &[FieldLayout], length_first: bool) -> S {
    let mut order: S = filled(0);
    let mut at = 0;
    while at < fields.len() {
        let mut hole = at;
        while hole > 0
            && precedes(
                identifier(names, fields[at]),
                identifier(names, fields[get::<u8, S>(&order, hole - 1) as usize]),
                length_first,
            )
        {
            let later = get::<u8, S>(&order, hole - 1);
            set(&mut order, hole, later);
            hole -= 1;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a layout has at most 64 entries"
        )]
        set(&mut order, hole, at as u8);
        at += 1;
    }
    order
}

/// `items` in the order [`sorted`] gives.
#[must_use]
pub const fn permuted<X: Copy, S: Storage<X>>(items: S, order: &[u8]) -> S {
    let mut out = items;
    let mut at = 0;
    while at < order.len() {
        set(&mut out, at, get::<X, S>(&items, order[at] as usize));
        at += 1;
    }
    out
}

/// Whether no two of `layout`'s entries are named alike, `skip`ped and `other` fields aside, whose
/// names no key can match; a struct and the structs it flattens share one map (#14 Q34).
#[must_use]
pub const fn distinct(layout: &Layout) -> bool {
    let mut a = 0;
    while a < layout.fields.len() {
        let name = identifier(layout.names, layout.fields[a]);
        let mut b = a + 1;
        while b < layout.fields.len() {
            let other = identifier(layout.names, layout.fields[b]);
            if !reserved(name) && name.len() == other.len() && holds(other, 0, name) {
                return false;
            }
            b += 1;
        }
        a += 1;
    }
    true
}

const fn identifier(names: &[u8], place: FieldLayout) -> &[u8] {
    let name_at = place.name_at as usize;
    names
        .split_at(name_at + place.name_len as usize)
        .0
        .split_at(name_at - head_before(place.name_len))
        .1
}

const fn precedes(a: &[u8], b: &[u8], length_first: bool) -> bool {
    if length_first && a.len() != b.len() {
        a.len() < b.len()
    } else {
        bytewise_less(a, b)
    }
}

const fn bytewise_less(a: &[u8], b: &[u8]) -> bool {
    match (a.split_first(), b.split_first()) {
        (Some((x, a)), Some((y, b))) if *x == *y => bytewise_less(a, b),
        (Some((x, _)), Some((y, _))) => *x < *y,
        (None, b) => b.is_some(),
        (Some(_), None) => false,
    }
}

/// The decode ops for a map of `layout`'s entries: matching only the key kinds they name, an integer
/// or a text, where a `skip`ped or `other` field's name counts as neither; and skipping, with `deny`
/// rejecting, or with an `other` field taking, unknown entries (D-05a, D-05b).
///
/// # Panics
/// At compile time, for more than one `other` field, or one beside `deny`.
#[must_use]
pub const fn map_ops(layout: &Layout, deny: bool) -> &'static DecOps {
    let (mut int, mut text, mut at) = (false, false, 0);
    while at < layout.fields.len() {
        let name = identifier(layout.names, layout.fields[at]);
        match Major::of(name[0]) {
            Major::Uint | Major::Nint => int = true,
            Major::Text => text |= !reserved(name),
            Major::Bytes | Major::Array | Major::Map | Major::Tag | Major::Simple => {}
        }
        at += 1;
    }
    match (int, text, deny, other(layout)) {
        (_, _, true, true) => panic!(
            "cbor-static: an `other` field takes the unknown entries `deny_unknown_fields` rejects"
        ),
        (false, _, false, false) => &DECODE_MAP,
        (false, _, true, false) => &DECODE_MAP_DENY,
        (false, _, false, true) => &DECODE_MAP_OTHER,
        (true, false, false, false) => &DECODE_MAP_INT,
        (true, false, true, false) => &DECODE_MAP_INT_DENY,
        (true, false, false, true) => &DECODE_MAP_INT_OTHER,
        (true, true, false, false) => &DECODE_MAP_MIXED,
        (true, true, true, false) => &DECODE_MAP_MIXED_DENY,
        (true, true, false, true) => &DECODE_MAP_MIXED_OTHER,
    }
}

/// The encode ops for a map of `layout`'s entries: writing an `other` field's entries in its place,
/// with no key of its own, when it has one (D-05b).
///
/// # Panics
/// At compile time, for more than one `other` field.
#[must_use]
pub const fn encode_ops(layout: &Layout) -> &'static EncOps {
    if other(layout) {
        &ENCODE_MAP_OTHER
    } else {
        &ENCODE_MAP
    }
}

/// The entry of `layout`'s `other` field, if it has one, found as the engine runs, so without the
/// const helpers' panics.
pub(crate) fn other_field(layout: &Layout) -> Option<u8> {
    const OTHER: [u8; names_len(&[Key::Other])] = names(&[Key::Other]);
    (0..layout.fields.len())
        .filter_map(|at| u8::try_from(at).ok())
        .find(|&at| layout.name_item(at) == OTHER)
}

/// Whether a descriptor's layout has an `other` field. An internally tagged enum's tag shares its
/// payload's map, so a payload's `other` field would take the tag entry (R-128).
#[must_use]
pub const fn holds_other(layout: Option<&Layout>) -> bool {
    match layout {
        None => false,
        Some(layout) => other(layout),
    }
}

/// Whether `layout` has an `other` field: one at most, a flattened struct's included.
const fn other(layout: &Layout) -> bool {
    let (mut found, mut at) = (false, 0);
    while at < layout.fields.len() {
        let name = identifier(layout.names, layout.fields[at]);
        if named(name, &Key::Other) {
            assert!(
                !found,
                "cbor-static: a struct takes one `other` field, its flattened fields' included"
            );
            found = true;
        }
        at += 1;
    }
    found
}

/// Whether `name` is a `skip`ped or an `other` field's, which no key can match.
const fn reserved(name: &[u8]) -> bool {
    named(name, &Key::Skipped) || named(name, &Key::Other)
}

/// Whether `name` is the identifier of `reserved`, a two-byte text no key can match.
const fn named(name: &[u8], reserved: &Key) -> bool {
    let keys = core::slice::from_ref(reserved);
    name.len() == names_len(keys) && holds(name, 0, &names::<2>(keys))
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
        Key::Skipped | Key::Other => Some(Head::new(Major::Text, 1)),
        Key::Lead(_) | Key::None => None,
    }
}

const fn payload(key: &Key) -> &[u8] {
    match key {
        Key::Text(text) => text.as_bytes(),
        Key::Int(_) | Key::Tag(_) | Key::None => &[],
        Key::Lead(_) => &[0; Lead::ENCODED_LEN],
        Key::Skipped => &[0xff],
        Key::Other => &[0xfe],
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
        Key::Text(_) | Key::Skipped | Key::Other => head_len(key),
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
    use super::{Key, field, map_ops, names, names_len, permuted, sorted};
    use crate::engine::{
        DECODE_MAP, DECODE_MAP_DENY, DECODE_MAP_INT, DECODE_MAP_INT_DENY, DECODE_MAP_MIXED,
        DECODE_MAP_MIXED_DENY, DecOps,
    };
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

    #[test]
    fn entries_sort_bytewise_or_shorter_first_by_their_whole_encodings() {
        const UNSORTED: &[Key] = &[
            Key::Text("aa"),
            Key::Int(-1),
            Key::Text("a twenty-four byte name!"),
            Key::Text("z"),
            Key::Int(100),
            Key::Int(10),
        ];
        const UNSORTED_NAMES: [u8; names_len(UNSORTED)] = names(UNSORTED);
        const FIELDS: [crate::schema::FieldLayout; 6] = [
            field(UNSORTED, 0, 0, false),
            field(UNSORTED, 1, 0, false),
            field(UNSORTED, 2, 0, false),
            field(UNSORTED, 3, 0, false),
            field(UNSORTED, 4, 0, false),
            field(UNSORTED, 5, 0, false),
        ];
        assert_eq!(
            sorted::<[u8; 6]>(&UNSORTED_NAMES, &FIELDS, false),
            [5, 4, 1, 3, 0, 2]
        );
        assert_eq!(
            sorted::<[u8; 6]>(&UNSORTED_NAMES, &FIELDS, true),
            [5, 1, 4, 3, 0, 2]
        );
    }

    #[test]
    fn entries_that_encode_alike_keep_their_order() {
        const TWICE: &[Key] = &[Key::Text("b"), Key::Text("a"), Key::Text("b")];
        const TWICE_NAMES: [u8; names_len(TWICE)] = names(TWICE);
        const FIELDS: [crate::schema::FieldLayout; 3] = [
            field(TWICE, 0, 0, false),
            field(TWICE, 1, 0, false),
            field(TWICE, 2, 0, false),
        ];
        assert_eq!(sorted::<[u8; 3]>(&TWICE_NAMES, &FIELDS, false), [1, 0, 2]);
    }

    #[test]
    fn items_take_the_order_given() {
        assert_eq!(permuted(['a', 'b', 'c'], &[2, 0, 1]), ['c', 'a', 'b']);
    }

    /// A layout of `keys`, at no offsets.
    macro_rules! layout_of {
        ($($key:expr),*) => {{
            const KEYS: &[Key] = &[$($key),*];
            const NAMES: [u8; names_len(KEYS)] = names(KEYS);
            const FIELDS: &[crate::schema::FieldLayout] = &{
                let mut fields = [field(KEYS, 0, 0, false); KEYS.len()];
                let mut at = 0;
                while at < KEYS.len() {
                    fields[at] = field(KEYS, at, 0, false);
                    at += 1;
                }
                fields
            };
            Layout { names: &NAMES, fields: FIELDS }
        }};
    }

    #[test]
    fn a_map_links_only_the_key_matching_its_names_need() {
        let cases: [(Layout, [&DecOps; 2]); 5] = [
            (
                layout_of!(Key::Text("a"), Key::Skipped),
                [&DECODE_MAP, &DECODE_MAP_DENY],
            ),
            (layout_of!(Key::Skipped), [&DECODE_MAP, &DECODE_MAP_DENY]),
            (
                layout_of!(Key::Int(-1), Key::Int(1000)),
                [&DECODE_MAP_INT, &DECODE_MAP_INT_DENY],
            ),
            (
                layout_of!(Key::Int(1), Key::Skipped),
                [&DECODE_MAP_INT, &DECODE_MAP_INT_DENY],
            ),
            (
                layout_of!(Key::Int(1), Key::Text("a")),
                [&DECODE_MAP_MIXED, &DECODE_MAP_MIXED_DENY],
            ),
        ];
        for (layout, [skip, deny]) in cases {
            assert!(core::ptr::eq(map_ops(&layout, false), skip));
            assert!(core::ptr::eq(map_ops(&layout, true), deny));
        }
    }

    #[test]
    fn names_differ_unless_both_are_skipped() {
        use super::distinct;

        assert!(distinct(&layout_of!(
            Key::Text("a"),
            Key::Int(1),
            Key::Skipped,
            Key::Skipped
        )));
        assert!(!distinct(&layout_of!(
            Key::Text("a"),
            Key::Skipped,
            Key::Text("a")
        )));
        assert!(!distinct(&layout_of!(Key::Int(24), Key::Int(24))));
        assert!(distinct(&layout_of!(
            Key::Int(24),
            Key::Int(-25),
            Key::Text("\x18")
        )));
    }
}
