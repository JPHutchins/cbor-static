use super::bit;
use super::text::Utf8;
use crate::error::ErrorKind;
use crate::wire::{Header, Major};

const MAX_INDEFINITE: u8 = 64;
const MAX_SAVED: usize = 8;

/// Iterative skip (D-14a): one merged count for definite nesting, a bitset over open
/// indefinite levels marking which saved an outer count, and a LIFO of those counts.
pub struct Skip {
    pending: u64,
    indefinite: u64,
    levels: u8,
    saved: [u64; MAX_SAVED],
    saved_len: u8,
    payload_left: u64,
    text: bool,
    chunked: Option<Major>,
    utf8: Utf8,
}

impl Skip {
    pub const fn new() -> Self {
        Self {
            pending: 0,
            indefinite: 0,
            levels: 0,
            saved: [0; MAX_SAVED],
            saved_len: 0,
            payload_left: 0,
            text: false,
            chunked: None,
            utf8: Utf8::new(),
        }
    }

    pub fn start(&mut self, items: u64) {
        *self = Self {
            pending: items,
            ..Self::new()
        };
    }

    pub const fn done(&self) -> bool {
        self.pending == 0 && self.levels == 0 && self.chunked.is_none() && self.payload_left == 0
    }

    pub const fn payload_left(&self) -> u64 {
        self.payload_left
    }

    pub fn payload(&mut self, segment: &[u8]) -> Result<(), ErrorKind> {
        if self.text {
            self.utf8.feed(segment)?;
        }
        self.payload_left -= u64::try_from(segment.len()).unwrap_or(self.payload_left);
        if self.payload_left == 0 && self.text {
            self.text = false;
            self.utf8.finish()?;
        }
        Ok(())
    }

    #[inline(never)]
    pub fn on_header(&mut self, header: Header) -> Result<(), ErrorKind> {
        if let Some(major) = self.chunked {
            return match (
                header.is_break(),
                header.major == major && !header.indefinite(),
            ) {
                (true, _) => {
                    self.chunked = None;
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
        self.pending = self.pending.saturating_sub(1);
        match (header.major, header.indefinite()) {
            (Major::Uint | Major::Nint | Major::Simple, _) => Ok(()),
            (Major::Tag, _) => self.add(1),
            (Major::Bytes | Major::Text, false) => {
                self.string(header);
                Ok(())
            }
            (Major::Bytes | Major::Text, true) => {
                self.chunked = Some(header.major);
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
        self.pending = self
            .pending
            .checked_add(items)
            .ok_or(ErrorKind::LimitExceeded)?;
        Ok(())
    }

    fn string(&mut self, header: Header) {
        self.payload_left = header.arg;
        self.text = header.major == Major::Text;
    }

    fn open(&mut self) -> Result<(), ErrorKind> {
        if self.levels == MAX_INDEFINITE {
            return Err(ErrorKind::LimitExceeded);
        }
        if self.pending > 0 {
            *self
                .saved
                .get_mut(usize::from(self.saved_len))
                .ok_or(ErrorKind::LimitExceeded)? = self.pending;
            self.saved_len += 1;
            self.indefinite |= bit(self.levels);
        }
        self.levels += 1;
        self.pending = 0;
        Ok(())
    }

    fn close(&mut self) -> Result<(), ErrorKind> {
        if self.levels == 0 || self.pending != 0 {
            return Err(ErrorKind::NotWellFormed);
        }
        self.levels -= 1;
        let level = bit(self.levels);
        self.pending = if self.indefinite & level == 0 {
            0
        } else {
            self.indefinite &= !level;
            self.saved_len -= 1;
            self.saved
                .get(usize::from(self.saved_len))
                .copied()
                .unwrap_or(0)
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Skip;
    use crate::error::ErrorKind;
    use crate::wire::{header_len, parse};

    fn skip(bytes: &[u8]) -> Result<usize, ErrorKind> {
        let mut state = Skip::new();
        state.start(1);
        let mut at = 0;
        while !state.done() {
            let wanted = usize::try_from(state.payload_left()).unwrap_or(usize::MAX);
            if wanted > 0 {
                let segment = bytes.get(at..at + wanted).ok_or(ErrorKind::UnexpectedEnd)?;
                state.payload(segment)?;
                at += wanted;
                continue;
            }
            let len = header_len(*bytes.get(at).ok_or(ErrorKind::UnexpectedEnd)?)?;
            state.on_header(parse(
                bytes.get(at..at + len).ok_or(ErrorKind::UnexpectedEnd)?,
            )?)?;
            at += len;
        }
        Ok(at)
    }

    #[test]
    fn skips_exactly_one_item() {
        let cases: [&[u8]; 6] = [
            b"\x01\x02",
            b"\xa2\x61a\x9f\x01\x02\xff\x61b\x03\x04",
            b"\x9f\x9f\xff\x82\x01\x9f\xff\xff\x05",
            b"\x7f\x61a\x62bc\xff\x00",
            b"\xc2\x42\x01\x02\x00",
            b"\xbf\x61a\xbf\xff\xff\x07",
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
        assert_eq!(skip(eight_saved), Err(ErrorKind::UnexpectedEnd));
        assert_eq!(
            skip(&[eight_saved.as_slice(), b"\x82\x9f"].concat()),
            Err(ErrorKind::LimitExceeded)
        );
    }
}
