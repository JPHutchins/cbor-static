use core::mem::MaybeUninit;

use super::bit;
use super::text::Utf8;
use crate::error::ErrorKind;
use crate::wire::{Header, Major, header_len, parse};

const MAX_INDEFINITE: u8 = 64;
const MAX_SAVED: usize = 8;

/// Iterative skip (D-14a): one merged count for definite nesting, a bitset over open
/// indefinite levels marking which saved an outer count, and a LIFO of those counts.
pub struct Skip {
    walk: Walk,
    indefinite: u64,
    saved: [MaybeUninit<u64>; MAX_SAVED],
}

/// The state a new skip resets. The open-level bitset and the saved counts are written before
/// they are read.
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
        self.walk.pending = self.walk.pending.saturating_sub(1);
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
            (Major::Array | Major::Map, true) => self.open(),
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

    fn open(&mut self) -> Result<(), ErrorKind> {
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
        self.walk.levels += 1;
        self.walk.pending = 0;
        Ok(())
    }

    fn close(&mut self) -> Result<(), ErrorKind> {
        if self.walk.levels == 0 || self.walk.pending != 0 {
            return Err(ErrorKind::NotWellFormed);
        }
        self.walk.levels -= 1;
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
pub fn length(bytes: &[u8]) -> Result<usize, ErrorKind> {
    let mut skip = Skip::started(1);
    let mut at = 0;
    while !skip.done() {
        let wanted = usize::try_from(skip.payload_left()).map_err(|_| ErrorKind::NeedContiguous)?;
        if wanted > 0 {
            skip.payload(
                bytes
                    .get(at..at + wanted)
                    .ok_or(ErrorKind::NeedContiguous)?,
            )?;
            at += wanted;
            continue;
        }
        let len = header_len(*bytes.get(at).ok_or(ErrorKind::NeedContiguous)?)?;
        skip.on_header(parse(
            bytes.get(at..at + len).ok_or(ErrorKind::NeedContiguous)?,
        )?)?;
        at += len;
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::length as skip;
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
    fn limits_saved_outer_counts() {
        let eight_saved = b"\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f\x82\x9f";
        assert_eq!(skip(eight_saved), Err(ErrorKind::NeedContiguous));
        assert_eq!(
            skip(&[eight_saved.as_slice(), b"\x82\x9f"].concat()),
            Err(ErrorKind::LimitExceeded)
        );
    }
}
