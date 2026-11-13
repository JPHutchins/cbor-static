#![forbid(unsafe_code)]
//! The classes of initial byte an item can start with, and for a tag, which tag number. An untagged
//! enum is restricted to variants whose leads are disjoint, so it picks its variant from the item's
//! head alone (D-10, #13 Q9).

use crate::wire::{Major, NULL, float};

/// A set of initial-byte classes; within the tag class, either one tag number or any.
///
/// ```
/// use cbor_static::__private::Lead;
///
/// assert!(Lead::of(0x05).overlaps(Lead::UINT));
/// assert!(Lead::of(0xf6).overlaps(Lead::NULL));
/// assert!(!Lead::UINT.union(Lead::NINT).overlaps(Lead::TEXT));
/// assert!(!Lead::tagged(0).overlaps(Lead::tagged(1)));
/// assert!(Lead::tagged(0).overlaps(Lead::TAG));
/// assert!(Lead::tagged(0).union(Lead::tagged(1)).overlaps(Lead::tagged(97)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lead {
    classes: u16,
    tag: Tag,
}

/// The tag numbers within the tag class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    Any,
    Only(u64),
}

impl Lead {
    pub const UINT: Self = Self::class(1 << Major::Uint as u8);
    pub const NINT: Self = Self::class(1 << Major::Nint as u8);
    pub const BYTES: Self = Self::class(1 << Major::Bytes as u8);
    pub const TEXT: Self = Self::class(1 << Major::Text as u8);
    pub const ARRAY: Self = Self::class(1 << Major::Array as u8);
    pub const MAP: Self = Self::class(1 << Major::Map as u8);
    /// Any tag number.
    pub const TAG: Self = Self::class(1 << Major::Tag as u8);
    pub const BOOL: Self = Self::class(1 << 7);
    pub const NULL: Self = Self::class(1 << 8);
    pub const FLOAT: Self = Self::class(1 << 9);
    pub const OTHER_SIMPLE: Self = Self::class(1 << 10);
    pub const ANY: Self = Self::class((1 << 11) - 1);
    pub const NONE: Self = Self::class(0);
    /// The bytes [`Lead::to_le_bytes`] writes.
    pub const ENCODED_LEN: usize = 11;

    const fn class(classes: u16) -> Self {
        Self {
            classes,
            tag: Tag::Any,
        }
    }

    /// The tag `number`, and no other.
    #[must_use]
    pub const fn tagged(number: u64) -> Self {
        Self {
            tag: Tag::Only(number),
            ..Self::TAG
        }
    }

    /// The class of an item starting with `initial`, any tag number included.
    #[must_use]
    pub const fn of(initial: u8) -> Self {
        match (initial >> 5, initial & 0x1f) {
            (7, 20 | 21) => Self::BOOL,
            (7, NULL) => Self::NULL,
            (7, float::HALF | float::SINGLE | float::DOUBLE) => Self::FLOAT,
            (7, _) => Self::OTHER_SIMPLE,
            (major, _) => Self::class(1 << major),
        }
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            classes: self.classes | other.classes,
            tag: match (self.has_tag(), other.has_tag(), self.tag, other.tag) {
                (true, false, tag, _) | (false, true, _, tag) => tag,
                (true, true, Tag::Only(a), Tag::Only(b)) if a == b => Tag::Only(a),
                _ => Tag::Any,
            },
        }
    }

    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.classes & other.classes & !Self::TAG.classes != 0
            || (self.has_tag()
                && other.has_tag()
                && !matches!((self.tag, other.tag), (Tag::Only(a), Tag::Only(b)) if a != b))
    }

    const fn has_tag(self) -> bool {
        self.classes & Self::TAG.classes != 0
    }

    /// The set as an untagged enum's layout stores it: the classes as two little-endian bytes,
    /// then 1 and the tag number as eight little-endian bytes for one tag, or nine zeros for any.
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; Self::ENCODED_LEN] {
        let [low, high] = self.classes.to_le_bytes();
        let (only, [t0, t1, t2, t3, t4, t5, t6, t7]) = match self.tag {
            Tag::Any => (0, [0; 8]),
            Tag::Only(number) => (1, number.to_le_bytes()),
        };
        [low, high, only, t0, t1, t2, t3, t4, t5, t6, t7]
    }

    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; Self::ENCODED_LEN]) -> Self {
        let [low, high, only, t0, t1, t2, t3, t4, t5, t6, t7] = bytes;
        Self {
            classes: u16::from_le_bytes([low, high]),
            tag: match only {
                0 => Tag::Any,
                _ => Tag::Only(u64::from_le_bytes([t0, t1, t2, t3, t4, t5, t6, t7])),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Lead;

    #[test]
    fn every_initial_byte_has_exactly_one_class() {
        let classes = [
            Lead::UINT,
            Lead::NINT,
            Lead::BYTES,
            Lead::TEXT,
            Lead::ARRAY,
            Lead::MAP,
            Lead::TAG,
            Lead::BOOL,
            Lead::NULL,
            Lead::FLOAT,
            Lead::OTHER_SIMPLE,
        ];
        for initial in 0..=u8::MAX {
            let of = Lead::of(initial);
            assert_eq!(
                classes.iter().filter(|class| of.overlaps(**class)).count(),
                1,
                "{initial:#04x}"
            );
            assert!(Lead::ANY.overlaps(of));
        }
        let expected = [
            (0x17, Lead::UINT),
            (0x38, Lead::NINT),
            (0x5f, Lead::BYTES),
            (0x7f, Lead::TEXT),
            (0x9f, Lead::ARRAY),
            (0xa0, Lead::MAP),
            (0xd2, Lead::TAG),
            (0xf4, Lead::BOOL),
            (0xf5, Lead::BOOL),
            (0xf6, Lead::NULL),
            (0xf7, Lead::OTHER_SIMPLE),
            (0xf9, Lead::FLOAT),
            (0xfb, Lead::FLOAT),
            (0xff, Lead::OTHER_SIMPLE),
        ];
        for (initial, class) in expected {
            assert_eq!(Lead::of(initial), class, "{initial:#04x}");
        }
    }

    #[test]
    fn tag_numbers_tell_tagged_leads_apart_until_a_union_mixes_them() {
        let (zero, one) = (Lead::tagged(0), Lead::tagged(1));
        assert!(!zero.overlaps(one));
        assert!(zero.overlaps(zero) && zero.overlaps(Lead::TAG) && Lead::TAG.overlaps(one));
        assert!(!zero.union(Lead::UINT).overlaps(one));
        assert!(zero.union(Lead::UINT).overlaps(Lead::of(0x01)));
        assert_eq!(zero.union(Lead::UINT).union(zero), zero.union(Lead::UINT));
        assert_eq!(zero.union(one), Lead::TAG);
        assert!(!Lead::UINT.union(Lead::TEXT).overlaps(Lead::TAG));
        for lead in [
            Lead::NONE,
            Lead::ANY,
            zero,
            Lead::tagged(u64::MAX).union(Lead::MAP),
        ] {
            assert_eq!(Lead::from_le_bytes(lead.to_le_bytes()), lead);
        }
    }
}
