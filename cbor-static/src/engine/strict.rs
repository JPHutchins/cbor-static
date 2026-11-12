//! The `Strict` profile (D-12): a decoder with it accepts only what RFC 8949 §4.2.1's deterministic
//! encoding writes, and no entry a field does not claim. Its checks are reached only through
//! [`STRICT`], which only `Decoder::new_strict` names, so firmware without it links none (D-22).

use core::mem::MaybeUninit;

use crate::error::ErrorKind;
#[cfg(feature = "heapless")]
use crate::schema::EncKind;
use crate::schema::Layout;
use crate::stack::Frame;
use crate::wire::Header;

/// The containers a raw item may nest, which `Strict`'s lack of indefinite lengths leaves the
/// skip's saved counts free to track (D-14a).
const MAX_OPEN: usize = 8;

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
    /// A raw item, whole: its every head, and the key order of its every map (#12 Q29).
    pub(super) raw: fn(&[u8]) -> Result<(), ErrorKind>,
    /// Two keys a map decoded, of one kind, whose encodings must rise bytewise; with the frames
    /// free above the map, which encoding them needs (#12 Q29).
    #[cfg(feature = "heapless")]
    pub(super) keys: Keys,
}

#[cfg(feature = "heapless")]
type Keys =
    fn(&'static EncKind, *const u8, *const u8, &mut [MaybeUninit<Frame>]) -> Result<(), ErrorKind>;

pub static STRICT: StrictOps = StrictOps {
    head,
    order,
    bignum,
    unknown,
    sorted,
    raw,
    #[cfg(feature = "heapless")]
    keys,
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

/// A container the raw walk is inside: its items left, whether it is a map, and where its previous
/// key began.
#[derive(Clone, Copy)]
struct Open {
    left: u64,
    map: bool,
    key: Option<usize>,
}

fn raw(item: &[u8]) -> Result<(), ErrorKind> {
    use super::skip::item_len;
    use crate::wire::{Major, header_len};

    let mut open = [Open {
        left: 1,
        map: false,
        key: None,
    }; MAX_OPEN + 1];
    let mut depth = 0;
    let mut at = 0;
    let mut tagged = false;
    while depth > 0 || open.first().is_some_and(|root| root.left > 0) {
        let rest = item.get(at..).unwrap_or(&[]);
        let len = header_len(*rest.first().ok_or(ErrorKind::NeedContiguous)?)?;
        let header = head(rest.get(..len).ok_or(ErrorKind::NeedContiguous)?)?;
        let level = open.get_mut(depth).ok_or(ErrorKind::Unsupported)?;
        if level.map && level.left % 2 == 0 && !tagged {
            let key = rest.get(..item_len(rest)?).unwrap_or(&[]);
            if let Some(previous) = level.key {
                let earlier = item.get(previous..).unwrap_or(&[]);
                deterministic(earlier.get(..item_len(earlier)?).unwrap_or(&[]) < key)?;
            }
            level.key = Some(at);
        }
        at += len;
        tagged = header.major == Major::Tag;
        let items = match header.major {
            Major::Tag => continue,
            Major::Bytes | Major::Text => {
                at += usize::try_from(header.arg).map_err(|_| ErrorKind::LimitExceeded)?;
                0
            }
            Major::Array => header.arg,
            Major::Map => header.arg.checked_mul(2).ok_or(ErrorKind::LimitExceeded)?,
            Major::Uint | Major::Nint | Major::Simple => 0,
        };
        level.left -= 1;
        if items > 0 {
            depth += 1;
            *open.get_mut(depth).ok_or(ErrorKind::LimitExceeded)? = Open {
                left: items,
                map: header.major == Major::Map,
                key: None,
            };
        }
        while depth > 0 && open.get(depth).is_some_and(|level| level.left == 0) {
            depth -= 1;
        }
    }
    Ok(())
}

/// Encodes both keys a byte at a time, each on half the free frames, and compares as they go.
#[cfg(feature = "heapless")]
fn keys(
    kind: &'static EncKind,
    earlier: *const u8,
    later: *const u8,
    frames: &mut [MaybeUninit<Frame>],
) -> Result<(), ErrorKind> {
    use super::encode::State;

    let (for_earlier, for_later) = frames.split_at_mut(frames.len() / 2);
    let (mut earlier, mut later) = (State::new(kind, earlier), State::new(kind, later));
    loop {
        match (
            byte(&mut earlier, for_earlier)?,
            byte(&mut later, for_later)?,
        ) {
            (Some(a), Some(b)) if a == b => {}
            (a, b) => return deterministic(a < b),
        }
    }
}

/// The next byte of an encoding, or `None` past its end.
#[cfg(feature = "heapless")]
fn byte(
    encoding: &mut super::encode::State,
    frames: &mut [MaybeUninit<Frame>],
) -> Result<Option<u8>, ErrorKind> {
    use super::Progress;
    use super::encode::resume;

    let mut one = [0];
    match resume(encoding, frames, &mut one).map_err(|error| error.kind)? {
        Progress::Done(0) => Ok(None),
        Progress::Done(_) | Progress::NeedSpace(_) => Ok(one.first().copied()),
    }
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
