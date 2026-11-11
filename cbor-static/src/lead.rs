#![forbid(unsafe_code)]
//! The classes of initial byte an item can start with. An untagged enum is restricted to variants
//! whose classes are disjoint, so it picks its variant from the initial byte alone (D-10).

use crate::wire::{Major, NULL, float};

/// A set of initial-byte classes.
///
/// ```
/// use cbor_static::__private::Lead;
///
/// assert!(Lead::of(0x05).overlaps(Lead::UINT));
/// assert!(Lead::of(0xf6).overlaps(Lead::NULL));
/// assert!(!Lead::UINT.union(Lead::NINT).overlaps(Lead::TEXT));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lead(u16);

impl Lead {
    pub const UINT: Self = Self(1 << Major::Uint as u8);
    pub const NINT: Self = Self(1 << Major::Nint as u8);
    pub const BYTES: Self = Self(1 << Major::Bytes as u8);
    pub const TEXT: Self = Self(1 << Major::Text as u8);
    pub const ARRAY: Self = Self(1 << Major::Array as u8);
    pub const MAP: Self = Self(1 << Major::Map as u8);
    pub const TAG: Self = Self(1 << Major::Tag as u8);
    pub const BOOL: Self = Self(1 << 7);
    pub const NULL: Self = Self(1 << 8);
    pub const FLOAT: Self = Self(1 << 9);
    pub const OTHER_SIMPLE: Self = Self(1 << 10);
    pub const ANY: Self = Self((1 << 11) - 1);
    pub const NONE: Self = Self(0);

    /// The class of an item starting with `initial`.
    #[must_use]
    pub const fn of(initial: u8) -> Self {
        match (initial >> 5, initial & 0x1f) {
            (7, 20 | 21) => Self::BOOL,
            (7, NULL) => Self::NULL,
            (7, float::HALF | float::SINGLE | float::DOUBLE) => Self::FLOAT,
            (7, _) => Self::OTHER_SIMPLE,
            (major, _) => Self(1 << major),
        }
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// The set as two little-endian bytes, as an untagged enum's layout stores it.
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
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
}
