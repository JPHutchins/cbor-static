use core::mem::MaybeUninit;

use super::bit;
use super::input::Input;
use super::text::Utf8;
use crate::error::ErrorKind;
use crate::wire::{Header, Major, length};

const MAX_INDEFINITE: u8 = 64;
const MAX_SAVED: usize = 8;

/// Iterative skip (D-14a, D-14b): one merged count for definite nesting, a bitset over open
/// indefinite levels marking which saved an outer count, a LIFO of those counts, and a stack of
/// flags marking which open levels are maps, whose keys each owe a value.
#[derive(Clone, Copy, Debug)]
pub struct Skip {
    walk: Walk,
    indefinite: u64,
    /// One bit per open indefinite level, the innermost in bit 0: set for a map.
    maps: u64,
    saved: [MaybeUninit<u64>; MAX_SAVED],
}

/// The state a new skip resets. The open-level bitset and the saved counts are written before
/// they are read.
#[derive(Clone, Copy, Debug)]
struct Walk {
    pending: u64,
    levels: u8,
    saved_len: u8,
    payload_left: u64,
    text: bool,
    chunked: Option<Major>,
    utf8: Utf8,
}

impl Walk {
    const fn new(items: u64) -> Self {
        Self {
            pending: items,
            levels: 0,
            saved_len: 0,
            payload_left: 0,
            text: false,
            chunked: None,
            utf8: Utf8::new(),
        }
    }
}

impl Skip {
    pub const fn started(items: u64) -> Self {
        Self {
            walk: Walk::new(items),
            indefinite: 0,
            maps: 0,
            saved: [MaybeUninit::uninit(); MAX_SAVED],
        }
    }

    pub const fn done(&self) -> bool {
        self.walk.pending == 0
            && self.walk.levels == 0
            && self.walk.chunked.is_none()
            && self.walk.payload_left == 0
    }

    pub const fn payload_left(&self) -> u64 {
        self.walk.payload_left
    }

    pub fn payload(&mut self, segment: &[u8]) -> Result<(), ErrorKind> {
        if self.walk.text {
            self.walk.utf8.feed(segment)?;
        }
        self.walk.payload_left -= u64::try_from(segment.len()).unwrap_or(self.walk.payload_left);
        if self.walk.payload_left == 0 && self.walk.text {
            self.walk.text = false;
            self.walk.utf8.finish()?;
        }
        Ok(())
    }

    /// Ends the current string's payload, which the caller took whole and checked itself.
    pub const fn took_payload(&mut self) {
        self.walk.payload_left = 0;
        self.walk.text = false;
    }

    #[inline(never)]
    pub fn on_header(&mut self, header: Header) -> Result<(), ErrorKind> {
        if let Some(major) = self.walk.chunked {
            return match (
                header.is_break(),
                header.major == major && !header.indefinite(),
            ) {
                (true, _) => {
                    self.walk.chunked = None;
                    Ok(())
                }
                (false, true) => {
                    self.string(header);
                    Ok(())
                }
                (false, false) => Err(ErrorKind::NotWellFormed),
            };
        }
        if header.is_break() {
            return self.close();
        }
        self.walk.pending = match self.walk.pending {
            0 if self.maps & 1 == 1 => 1,
            pending => pending.saturating_sub(1),
        };
        match (header.major, header.indefinite()) {
            (Major::Uint | Major::Nint | Major::Simple, _) => Ok(()),
            (Major::Tag, _) => self.add(1),
            (Major::Bytes | Major::Text, false) => {
                self.string(header);
                Ok(())
            }
            (Major::Bytes | Major::Text, true) => {
                self.walk.chunked = Some(header.major);
                Ok(())
            }
            (Major::Array, false) => self.add(header.arg),
            (Major::Map, false) => {
                self.add(header.arg.checked_mul(2).ok_or(ErrorKind::LimitExceeded)?)
            }
            (Major::Array | Major::Map, true) => self.open(header.major == Major::Map),
        }
    }

    fn add(&mut self, items: u64) -> Result<(), ErrorKind> {
        self.walk.pending = self
            .walk
            .pending
            .checked_add(items)
            .ok_or(ErrorKind::LimitExceeded)?;
        Ok(())
    }

    fn string(&mut self, header: Header) {
        self.walk.payload_left = header.arg;
        self.walk.text = header.major == Major::Text;
    }

    fn open(&mut self, map: bool) -> Result<(), ErrorKind> {
        if self.walk.levels == MAX_INDEFINITE {
            return Err(ErrorKind::LimitExceeded);
        }
        let level = bit(self.walk.levels);
        if self.walk.pending > 0 {
            self.saved
                .get_mut(usize::from(self.walk.saved_len))
                .ok_or(ErrorKind::LimitExceeded)?
                .write(self.walk.pending);
            self.walk.saved_len += 1;
            self.indefinite |= level;
        } else {
            self.indefinite &= !level;
        }
        self.maps = self.maps << 1 | u64::from(map);
        self.walk.levels += 1;
        self.walk.pending = 0;
        Ok(())
    }

    fn close(&mut self) -> Result<(), ErrorKind> {
        if self.walk.levels == 0 || self.walk.pending != 0 {
            return Err(ErrorKind::NotWellFormed);
        }
        self.walk.levels -= 1;
        self.maps >>= 1;
        let level = bit(self.walk.levels);
        self.walk.pending = if self.indefinite & level == 0 {
            0
        } else {
            self.walk.saved_len -= 1;
            self.saved
                .get(usize::from(self.walk.saved_len))
                .map_or(0, |count| unsafe { count.assume_init_read() })
        };
        Ok(())
    }
}

/// The length of the one item `bytes` starts with, all of which must be there.
///
/// # Errors
/// [`ErrorKind::NeedContiguous`] when `bytes` ends inside the item; any error skipping it would.
pub fn item_len(bytes: &[u8]) -> Result<usize, ErrorKind> {
    let mut input = Input::new(bytes);
    let mut skip = Skip::started(1);
    while !skip.done() {
        match length(skip.payload_left())? {
            0 => skip.on_header(input.take_head()?.ok_or(ErrorKind::NeedContiguous)?)?,
            wanted => skip.payload(input.take_exact(wanted).ok_or(ErrorKind::NeedContiguous)?)?,
        }
    }
    Ok(input.at)
}

#[cfg(test)]
mod tests {
    use super::{Skip, item_len as skip};
    use crate::error::ErrorKind;

    #[test]
    fn skips_exactly_one_item() {
        let cases: [&[u8]; 7] = [
            b"\x01\x02",
            b"\xa2\x61a\x9f\x01\x02\xff\x61b\x03\x04",
            b"\x9f\x9f\xff\x82\x01\x9f\xff\xff\x05",
            b"\x7f\x61a\x62bc\xff\x00",
            b"\xc2\x42\x01\x02\x00",
            b"\xbf\x61a\xbf\xff\xff\x07",
            b"\x82\x9f\xff\x9f\xff\x00",
        ];
        for bytes in cases {
            assert_eq!(skip(bytes), Ok(bytes.len() - 1), "{bytes:x?}");
        }
    }

    #[test]
    fn rejects_not_well_formed_nesting() {
        let cases: [(&[u8], ErrorKind); 5] = [
            (b"\x9f\x82\x01\xff\x02", ErrorKind::NotWellFormed),
            (b"\xff", ErrorKind::NotWellFormed),
            (b"\x7f\x41a\xff", ErrorKind::NotWellFormed),
            (b"\x62\xc3\x28", ErrorKind::InvalidUtf8),
            (b"\x7f\x61\xc3\x61\xa9\xff", ErrorKind::InvalidUtf8),
        ];
        for (bytes, error) in cases {
            assert_eq!(skip(bytes), Err(error), "{bytes:x?}");
        }
    }

    #[test]
    fn indefinite_maps_pair_every_key_with_a_value() {
        let paired: [&[u8]; 6] = [
            b"\xbf\x82\x01\x02\x03\xff\x00",
            b"\xbf\x9f\xff\x01\xff\x00",
            b"\xbf\x01\x9f\xbf\xff\xff\xff\x00",
            b"\xbf\x7f\x61a\xff\x01\xff\x00",
            b"\xbf\x01\xbf\x02\x03\xff\xff\x00",
            b"\x82\xbf\xff\x9f\x01\xff\x00",
        ];
        for bytes in paired {
            assert_eq!(skip(bytes), Ok(bytes.len() - 1), "{bytes:x?}");
        }
        let ten_deep = [
            b"\xbf\x01".repeat(10),
            b"\xbf\xff".to_vec(),
            b"\xff".repeat(10),
            b"\x00".to_vec(),
        ]
        .concat();
        assert_eq!(skip(&ten_deep), Ok(ten_deep.len() - 1));
        let unpaired: [&[u8]; 7] = [
            b"\xbf\x01\x9f\xff\x02\xff",
            b"\xbf\x00\xff",
            b"\xbf\x00\x00\x00\xff",
            b"\xbf\x00\x01\x03\xff",
            b"\xbf\x9f\xff\xff",
            b"\x9f\xbf\x01\xff\xff",
            b"\xbf\x01\xbf\x02\xff\xff",
        ];
        for bytes in unpaired {
            assert_eq!(skip(bytes), Err(ErrorKind::NotWellFormed), "{bytes:x?}");
        }
    }

    #[test]
    fn the_state_is_104_bytes() {
        assert_eq!(size_of::<Skip>(), 104);
    }

    #[test]
    fn limits_saved_outer_counts() {
        let eight_saved = b"\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f";
        assert_eq!(skip(eight_saved), Err(ErrorKind::NeedContiguous));
        assert_eq!(
            skip(&[eight_saved.as_slice(), b"\x82\x9f"].concat()),
            Err(ErrorKind::LimitExceeded)
        );
    }
}

#[cfg(kani)]
mod proofs {
    use core::mem::MaybeUninit;

    use super::{MAX_INDEFINITE, MAX_SAVED, Skip, Walk};
    use crate::engine::text::Utf8;
    use crate::error::ErrorKind;
    use crate::wire::{Header, INDEFINITE, Major};

    const MAJORS: [Major; 8] = [
        Major::Uint,
        Major::Nint,
        Major::Bytes,
        Major::Text,
        Major::Array,
        Major::Map,
        Major::Tag,
        Major::Simple,
    ];
    const BREAK: Header = Header {
        major: Major::Simple,
        info: INDEFINITE,
        arg: 0,
    };

    /// Any header: the skip reads only its major type, whether it is indefinite, and its argument.
    fn any_header() -> Header {
        let (major, info): (usize, u8) = (kani::any(), kani::any());
        kani::assume(major < MAJORS.len() && (info < 28 || info == INDEFINITE));
        Header {
            major: MAJORS[major],
            info,
            arg: kani::any(),
        }
    }

    fn open(levels: u8) -> u64 {
        u64::MAX.checked_shr(64 - u32::from(levels)).unwrap_or(0)
    }

    /// The skip state's invariant between headers.
    fn holds(skip: &Skip) -> bool {
        skip.walk.levels <= MAX_INDEFINITE
            && skip.maps & !open(skip.walk.levels) == 0
            && usize::from(skip.walk.saved_len) <= MAX_SAVED
            && (skip.indefinite & open(skip.walk.levels)).count_ones()
                == u32::from(skip.walk.saved_len)
    }

    /// Any state that satisfies the invariant.
    fn any_skip() -> Skip {
        let chunked: bool = kani::any();
        let skip = Skip {
            walk: Walk {
                pending: kani::any(),
                levels: kani::any(),
                saved_len: kani::any(),
                payload_left: kani::any(),
                text: kani::any(),
                chunked: chunked.then(|| any_header().major),
                utf8: Utf8::new(),
            },
            indefinite: kani::any(),
            maps: kani::any(),
            saved: core::array::from_fn(|_| MaybeUninit::new(kani::any())),
        };
        kani::assume(holds(&skip));
        skip
    }

    #[kani::proof]
    fn every_header_keeps_the_invariant_without_overflow() {
        let mut skip = any_skip();
        let _ = skip.on_header(any_header());
        assert!(holds(&skip));
    }

    #[kani::proof]
    fn opening_past_64_levels_or_8_saved_counts_is_limit_exceeded() {
        let mut skip = any_skip();
        let header = any_header();
        kani::assume(
            skip.walk.chunked.is_none()
                && matches!(header.major, Major::Array | Major::Map)
                && header.indefinite(),
        );
        let saves = skip.walk.pending > 1 || (skip.walk.pending == 0 && skip.maps & 1 == 1);
        let full = skip.walk.levels == MAX_INDEFINITE
            || (saves && usize::from(skip.walk.saved_len) == MAX_SAVED);
        assert_eq!(skip.on_header(header).is_err(), full);
    }

    #[kani::proof]
    fn an_indefinite_map_closes_only_on_a_key_boundary() {
        let mut skip = any_skip();
        kani::assume(
            skip.walk.levels > 0
                && skip.maps & 1 == 1
                && skip.walk.pending == 0
                && skip.walk.chunked.is_none()
                && skip.walk.payload_left == 0,
        );
        let (key, value) = (any_header(), any_header());
        let scalar = |header: &Header| {
            matches!(header.major, Major::Uint | Major::Nint | Major::Simple) && !header.is_break()
        };
        kani::assume(scalar(&key) && scalar(&value));
        let mut unpaired = skip.clone();
        assert!(unpaired.on_header(key).is_ok());
        assert_eq!(unpaired.on_header(BREAK), Err(ErrorKind::NotWellFormed));
        assert!(skip.on_header(key).is_ok() && skip.on_header(value).is_ok());
        assert_eq!(skip.on_header(BREAK), Ok(()));
    }
}
