use super::bit;
use crate::error::ErrorKind;
use crate::schema::Layout;

const CONTINUATION_LOW: u8 = 0x80;
const CONTINUATION_HIGH: u8 = 0xbf;

/// UTF-8 validation (RFC 3629) across chunk boundaries: the continuation bytes still owed by the
/// current code point, and the range the next one must fall in.
#[derive(Clone, Copy, Debug)]
pub struct Utf8 {
    owed: u8,
    low: u8,
    high: u8,
}

/// Matches a text key, chunk by chunk, against a layout's names of the same length.
pub struct KeyMatch {
    left: u64,
    at: usize,
    candidates: u64,
    utf8: Utf8,
}

impl Utf8 {
    pub const fn new() -> Self {
        Self::expecting(0, CONTINUATION_LOW, CONTINUATION_HIGH)
    }

    const fn expecting(owed: u8, low: u8, high: u8) -> Self {
        Self { owed, low, high }
    }

    #[inline(never)]
    pub fn feed(&mut self, segment: &[u8]) -> Result<(), ErrorKind> {
        *self = segment
            .iter()
            .try_fold(*self, |state, &byte| state.next(byte))?;
        Ok(())
    }

    pub const fn finish(self) -> Result<(), ErrorKind> {
        match self.owed {
            0 => Ok(()),
            _ => Err(ErrorKind::InvalidUtf8),
        }
    }

    /// RFC 3629 Table 3-7: the lead byte fixes how many continuation bytes follow and, to exclude
    /// overlong forms, surrogates and code points above U+10FFFF, the range of the first one.
    const fn next(self, byte: u8) -> Result<Self, ErrorKind> {
        match (self.owed, byte) {
            (0, 0x00..=0x7f) => Ok(self),
            (0, 0xc2..=0xdf) => Ok(Self::expecting(1, CONTINUATION_LOW, CONTINUATION_HIGH)),
            (0, 0xe0) => Ok(Self::expecting(2, 0xa0, CONTINUATION_HIGH)),
            (0, 0xe1..=0xec | 0xee..=0xef) => {
                Ok(Self::expecting(2, CONTINUATION_LOW, CONTINUATION_HIGH))
            }
            (0, 0xed) => Ok(Self::expecting(2, CONTINUATION_LOW, 0x9f)),
            (0, 0xf0) => Ok(Self::expecting(3, 0x90, CONTINUATION_HIGH)),
            (0, 0xf1..=0xf3) => Ok(Self::expecting(3, CONTINUATION_LOW, CONTINUATION_HIGH)),
            (0, 0xf4) => Ok(Self::expecting(3, CONTINUATION_LOW, 0x8f)),
            (owed @ 1.., byte) if self.low <= byte && byte <= self.high => Ok(Self::expecting(
                owed - 1,
                CONTINUATION_LOW,
                CONTINUATION_HIGH,
            )),
            _ => Err(ErrorKind::InvalidUtf8),
        }
    }
}

/// Validates a whole string.
pub fn validate(bytes: &[u8]) -> Result<(), ErrorKind> {
    let mut utf8 = Utf8::new();
    utf8.feed(bytes)?;
    utf8.finish()
}

/// A whole string, once validated.
pub fn utf8(bytes: &[u8]) -> Result<&str, ErrorKind> {
    validate(bytes).map(|()| unsafe { core::str::from_utf8_unchecked(bytes) })
}

impl KeyMatch {
    pub fn start(layout: &Layout, len: u64) -> Self {
        Self {
            left: len,
            at: 0,
            candidates: names(layout)
                .filter(|&(_, place)| u64::from(place.name_len) == len)
                .fold(0, |mask, (index, _)| mask | bit(index)),
            utf8: Utf8::new(),
        }
    }

    /// Never matches the `entries` set in the mask.
    pub const fn excluding(self, entries: u64) -> Self {
        Self {
            candidates: self.candidates & !entries,
            ..self
        }
    }

    #[inline(never)]
    pub fn feed(&mut self, layout: &Layout, segment: &[u8]) -> Result<(), ErrorKind> {
        self.utf8.feed(segment)?;
        self.candidates = names(layout)
            .filter(|&(index, place)| {
                self.candidates & bit(index) != 0
                    && layout
                        .names
                        .get(usize::from(place.name_at) + self.at..)
                        .and_then(|n| n.get(..segment.len()))
                        == Some(segment)
            })
            .fold(0, |mask, (index, _)| mask | bit(index));
        self.at += segment.len();
        self.left -= u64::try_from(segment.len()).unwrap_or(self.left);
        Ok(())
    }

    pub const fn left(&self) -> u64 {
        self.left
    }

    pub fn finish(&mut self) -> Result<Option<u8>, ErrorKind> {
        self.utf8.finish()?;
        Ok((self.candidates != 0)
            .then(|| u8::try_from(self.candidates.trailing_zeros()).unwrap_or(u8::MAX)))
    }
}

fn names(layout: &Layout) -> impl Iterator<Item = (u8, &crate::schema::FieldLayout)> {
    layout
        .fields
        .iter()
        .enumerate()
        .filter_map(|(index, place)| Some((u8::try_from(index).ok()?, place)))
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::{Utf8, validate};
    use crate::error::ErrorKind;

    /// Every edge of RFC 3629 Table 3-7, plus bytes that are never valid.
    const BOUNDARIES: [u8; 24] = [
        0x00, 0x7f, 0x80, 0x8f, 0x90, 0x9f, 0xa0, 0xbf, 0xc0, 0xc1, 0xc2, 0xdf, 0xe0, 0xe1, 0xec,
        0xed, 0xee, 0xef, 0xf0, 0xf1, 0xf3, 0xf4, 0xf5, 0xff,
    ];

    fn chunked(bytes: &[u8], split: usize) -> Result<(), ErrorKind> {
        let (head, tail) = bytes.split_at(split);
        let mut utf8 = Utf8::new();
        utf8.feed(head)?;
        utf8.feed(tail)?;
        utf8.finish()
    }

    #[test]
    fn accepts_every_width_and_rejects_the_rfc_3629_exclusions() {
        let valid: [&[u8]; 6] = [
            b"a",
            b"\xc3\xa9",
            b"\xe2\x82\xac",
            b"\xed\x9f\xbf",
            b"\xf0\x90\x8d\x88",
            b"\xf4\x8f\xbf\xbf",
        ];
        let invalid: [&[u8]; 8] = [
            b"\x80",
            b"\xc0\x80",
            b"\xe0\x80\xaf",
            b"\xed\xa0\x80",
            b"\xf0\x80\x80\xaf",
            b"\xf4\x90\x80\x80",
            b"\xf5\x80\x80\x80",
            b"\xe2\x82",
        ];
        for bytes in valid {
            assert_eq!(validate(bytes), Ok(()), "{bytes:x?}");
            assert!((0..=bytes.len()).all(|split| chunked(bytes, split).is_ok()));
        }
        for bytes in invalid {
            assert_eq!(validate(bytes), Err(ErrorKind::InvalidUtf8), "{bytes:x?}");
            assert!((0..=bytes.len()).all(|split| chunked(bytes, split).is_err()));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "331,776 sequences")]
    fn agrees_with_core_on_every_boundary_sequence_at_every_split() {
        (1..=4u32)
            .flat_map(|len| {
                (0..BOUNDARIES.len().pow(len)).map(move |index| {
                    (0..len)
                        .scan(index, |rest, _| {
                            let byte = BOUNDARIES[*rest % BOUNDARIES.len()];
                            *rest /= BOUNDARIES.len();
                            Some(byte)
                        })
                        .collect::<Vec<u8>>()
                })
            })
            .for_each(|bytes| {
                let expected = core::str::from_utf8(&bytes).is_ok();
                assert_eq!(validate(&bytes).is_ok(), expected, "{bytes:x?}");
                assert!(
                    (0..=bytes.len()).all(|split| chunked(&bytes, split).is_ok() == expected),
                    "{bytes:x?}"
                );
            });
    }
}
