use super::bit;
use crate::error::ErrorKind;
use crate::schema::Layout;

/// UTF-8 validation across chunk boundaries: carries an incomplete code point (≤ 3 bytes).
pub struct Utf8 {
    partial: [u8; 4],
    len: u8,
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
        Self {
            partial: [0; 4],
            len: 0,
        }
    }

    #[inline(never)]
    pub fn feed(&mut self, segment: &[u8]) -> Result<(), ErrorKind> {
        let rest = self.complete(segment)?;
        match core::str::from_utf8(rest) {
            Ok(_) => Ok(()),
            Err(e) if e.error_len().is_none() && !cfg!(cbor_static_oneshot) => {
                self.stash(rest.get(e.valid_up_to()..).unwrap_or(&[]));
                Ok(())
            }
            Err(_) => Err(ErrorKind::InvalidUtf8),
        }
    }

    pub const fn finish(&mut self) -> Result<(), ErrorKind> {
        if self.len == 0 {
            Ok(())
        } else {
            self.len = 0;
            Err(ErrorKind::InvalidUtf8)
        }
    }

    fn complete<'a>(&mut self, segment: &'a [u8]) -> Result<&'a [u8], ErrorKind> {
        if self.len == 0 {
            return Ok(segment);
        }
        let width = match self.partial[0] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            _ => 4,
        };
        let (head, rest) = segment
            .split_at_checked((width - usize::from(self.len)).min(segment.len()))
            .unwrap_or((segment, &[]));
        self.stash(head);
        if usize::from(self.len) < width {
            return Ok(rest);
        }
        self.len = 0;
        core::str::from_utf8(self.partial.get(..width).unwrap_or(&[]))
            .map(|_| rest)
            .map_err(|_| ErrorKind::InvalidUtf8)
    }

    fn stash(&mut self, bytes: &[u8]) {
        let start = usize::from(self.len);
        let copied = self
            .partial
            .iter_mut()
            .skip(start)
            .zip(bytes)
            .map(|(d, s)| *d = *s)
            .count();
        self.len = u8::try_from(start + copied).unwrap_or(u8::MAX);
    }
}

impl KeyMatch {
    pub const fn new() -> Self {
        Self {
            left: 0,
            at: 0,
            candidates: 0,
            utf8: Utf8::new(),
        }
    }

    pub fn start(&mut self, layout: &Layout, len: u64) {
        *self = Self {
            left: len,
            candidates: names(layout)
                .filter(|&(_, place)| u64::from(place.name_len) == len)
                .fold(0, |mask, (index, _)| mask | bit(index)),
            ..Self::new()
        };
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
