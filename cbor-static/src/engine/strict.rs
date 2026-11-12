//! The `Strict` profile (D-12): a decoder with it accepts only what RFC 8949 §4.2.1's deterministic
//! encoding writes, and no entry a field does not claim. Its checks are reached only through
//! [`STRICT`], which only `Decoder::new_strict` names, so firmware without it links none (D-22).

use core::mem::MaybeUninit;

use crate::error::ErrorKind;
use crate::schema::Layout;
use crate::stack::Frame;
use crate::wire::Header;

/// One check per place `Strict` looks. Each rejects with [`ErrorKind::NotDeterministic`], or, for
/// an entry no field claims, [`ErrorKind::UnknownField`].
pub struct StrictOps {
    /// Every head read, which it parses.
    pub(super) head: fn(&[u8]) -> Result<Header, ErrorKind>,
    /// The field a map's frame takes next.
    pub(super) order: fn(&Layout, &Frame, u8) -> Result<(), ErrorKind>,
    pub(super) bignum: fn(&[u8]) -> Result<(), ErrorKind>,
    /// An entry the top frame of a depth does not claim.
    pub(super) unknown: fn(&mut [MaybeUninit<Frame>], u8) -> Result<(), ErrorKind>,
    /// A map held whole in this chunk, of a count of entries.
    pub(super) sorted: fn(&[u8], u64) -> Result<(), ErrorKind>,
}

pub static STRICT: StrictOps = StrictOps {
    head,
    order,
    bignum,
    unknown,
    sorted,
};

fn head(bytes: &[u8]) -> Result<Header, ErrorKind> {
    use crate::wire::{Head, INDEFINITE, Major, float, parse};

    let header = parse(bytes)?;
    deterministic(match (header.major, header.info) {
        (Major::Simple, INDEFINITE) => true,
        (_, INDEFINITE) => false,
        (Major::Simple, float::HALF | float::SINGLE | float::DOUBLE) => {
            float::deterministic(header.info, header.arg)
        }
        (Major::Simple, _) => true,
        (major, _) => bytes.len() == Head::new(major, header.arg).as_bytes().len(),
    })
    .map(|()| header)
}

fn order(layout: &Layout, frame: &Frame, field: u8) -> Result<(), ErrorKind> {
    deterministic(frame.seen == 0 || layout.name_item(frame.field) < layout.name_item(field))
}

fn bignum(digits: &[u8]) -> Result<(), ErrorKind> {
    deterministic(digits.len() > size_of::<u64>() && digits.first() != Some(&0))
}

fn unknown(frames: &mut [MaybeUninit<Frame>], depth: u8) -> Result<(), ErrorKind> {
    use super::frame_mut;
    use crate::stack::Phase;

    let frame = frame_mut(frames, depth.checked_sub(1).ok_or(ErrorKind::Unsupported)?)?;
    match frame.phase {
        Phase::TagAhead => {
            frame.phase = Phase::Key;
            Ok(())
        }
        _ => Err(ErrorKind::UnknownField),
    }
}

fn sorted(bytes: &[u8], count: u64) -> Result<(), ErrorKind> {
    use super::skip::item_len;

    (0..count)
        .try_fold((0, [].as_slice()), |(at, previous), _| {
            let rest = bytes.get(at..).unwrap_or(&[]);
            let key = rest.get(..item_len(rest)?).unwrap_or(&[]);
            deterministic(previous < key)?;
            Ok((
                at + key.len() + item_len(rest.get(key.len()..).unwrap_or(&[]))?,
                key,
            ))
        })
        .map(|_| ())
}

fn deterministic(holds: bool) -> Result<(), ErrorKind> {
    holds.then_some(()).ok_or(ErrorKind::NotDeterministic)
}

#[cfg(test)]
mod tests {
    use super::{bignum, head, order, sorted};
    use crate::engine::bit;
    use crate::error::ErrorKind;
    use crate::keys::{Key, field, names, names_len};
    use crate::schema::Layout;
    use crate::stack::{Frame, Phase};

    #[test]
    fn heads_must_take_their_preferred_length_and_be_definite() {
        let cases: [(&[u8], bool); 12] = [
            (b"\x17", true),
            (b"\x18\x17", false),
            (b"\x18\x18", true),
            (b"\x19\x00\xff", false),
            (b"\x39\x01\x00", true),
            (b"\x7a\x00\x00\x00\x01", false),
            (b"\x9b\x00\x00\x00\x01\x00\x00\x00\x00", true),
            (b"\xd8\x02", false),
            (b"\x5f", false),
            (b"\xbf", false),
            (b"\xf8\x20", true),
            (b"\xff", true),
        ];
        for (bytes, accepted) in cases {
            assert_eq!(
                head(bytes).map(|_| ()),
                if accepted {
                    Ok(())
                } else {
                    Err(ErrorKind::NotDeterministic)
                },
                "{bytes:x?}"
            );
        }
    }

    const KEYS: &[Key] = &[Key::Int(-1), Key::Int(24), Key::Text("b"), Key::Text("aa")];
    const NAMES: [u8; names_len(KEYS)] = names(KEYS);
    const LAYOUT: Layout = Layout {
        names: &NAMES,
        fields: &[
            field(KEYS, 0, 0, false),
            field(KEYS, 1, 0, false),
            field(KEYS, 2, 0, false),
            field(KEYS, 3, 0, false),
        ],
    };

    /// A map's frame after `previous`, if any.
    fn after(previous: Option<u8>) -> Frame {
        Frame {
            remaining: 0,
            seen: previous.map_or(0, bit),
            desc: core::ptr::null(),
            base: core::ptr::null_mut(),
            field: previous.unwrap_or(0),
            phase: Phase::Key,
        }
    }

    #[test]
    fn keys_rise_bytewise_by_their_encoding_head_included() {
        assert_eq!(order(&LAYOUT, &after(None), 3), Ok(()));
        for (previous, field) in [(1, 0), (1, 2), (0, 2), (2, 3), (1, 3)] {
            assert_eq!(order(&LAYOUT, &after(Some(previous)), field), Ok(()));
            assert_eq!(
                order(&LAYOUT, &after(Some(field)), previous),
                Err(ErrorKind::NotDeterministic)
            );
        }
        assert_eq!(
            order(&LAYOUT, &after(Some(2)), 2),
            Err(ErrorKind::NotDeterministic)
        );
    }

    #[test]
    fn a_bignum_has_no_leading_zero_and_outgrows_64_bits() {
        let cases: [(&[u8], bool); 5] = [
            (b"", false),
            (b"\x01", false),
            (b"\xff\xff\xff\xff\xff\xff\xff\xff", false),
            (b"\x01\x00\x00\x00\x00\x00\x00\x00\x00", true),
            (b"\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00", false),
        ];
        for (digits, accepted) in cases {
            assert_eq!(bignum(digits).is_ok(), accepted, "{digits:x?}");
        }
    }

    #[test]
    fn a_whole_map_has_rising_keys() {
        let cases: [(u64, &[u8], Result<(), ErrorKind>); 6] = [
            (0, b"", Ok(())),
            (3, b"\x18\x18\x01\x61b\x02\x62aa\x03", Ok(())),
            (2, b"\x62aa\x01\x61b\x02", Err(ErrorKind::NotDeterministic)),
            (2, b"\x61b\x01\x61b\x02", Err(ErrorKind::NotDeterministic)),
            (2, b"\x61a\x82\x01\x02\x61b\xa1\x00\x00", Ok(())),
            (2, b"\x61a\x01\x61b", Err(ErrorKind::NeedContiguous)),
        ];
        for (count, bytes, expected) in cases {
            assert_eq!(sorted(bytes, count), expected, "{bytes:x?}");
        }
    }
}
