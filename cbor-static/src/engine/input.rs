use core::mem::MaybeUninit;

use crate::error::ErrorKind;
use crate::wire::{Header, header_len, parse};

pub struct Input<'a> {
    bytes: &'a [u8],
    pub at: usize,
}

/// A header split across chunks: at most 9 bytes.
pub struct Carry {
    bytes: [MaybeUninit<u8>; 9],
    len: u8,
}

impl<'a> Input<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub fn rest(&self) -> &'a [u8] {
        self.bytes.get(self.at..).unwrap_or(&[])
    }

    pub fn take(&mut self, max: u64) -> &'a [u8] {
        let rest = self.rest();
        let taken = rest
            .get(..usize::try_from(max).map_or(rest.len(), |m| m.min(rest.len())))
            .unwrap_or(&[]);
        self.at += taken.len();
        taken
    }

    /// Exactly `len` bytes, or `None` while this chunk holds fewer.
    pub fn take_exact(&mut self, len: usize) -> Option<&'a [u8]> {
        let taken = self.rest().get(..len)?;
        self.at += taken.len();
        Some(taken)
    }

    pub fn take_rest(&mut self) -> &'a [u8] {
        let rest = self.rest();
        self.at += rest.len();
        rest
    }
}

impl Carry {
    pub const fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); 9],
            len: 0,
        }
    }

    /// The next item's initial byte, without taking it.
    pub fn initial(&self, input: &Input) -> Option<u8> {
        match self.len {
            0 => input.rest().first().copied(),
            _ => Some(unsafe { self.bytes[0].assume_init() }),
        }
    }

    /// The next header, or `None` until the chunks supplied so far complete one.
    #[inline(never)]
    pub fn header(&mut self, input: &mut Input) -> Result<Option<Header>, ErrorKind> {
        if cfg!(cbor_static_oneshot) || self.len == 0 {
            let Some(&initial) = input.rest().first() else {
                return Ok(None);
            };
            return match input.take_exact(header_len(initial)?) {
                Some(bytes) => parse(bytes).map(Some),
                None if cfg!(cbor_static_oneshot) => Err(ErrorKind::UnexpectedEnd),
                None => {
                    self.stash(input.take_rest());
                    Ok(None)
                }
            };
        }
        let need = header_len(unsafe { self.bytes[0].assume_init() })?;
        self.stash(input.take(u64::try_from(need - usize::from(self.len)).unwrap_or(0)));
        if usize::from(self.len) < need {
            return Ok(None);
        }
        self.len = 0;
        parse(unsafe { self.bytes.get(..need).unwrap_or(&[]).assume_init_ref() }).map(Some)
    }

    fn stash(&mut self, bytes: &[u8]) {
        let start = usize::from(self.len);
        let copied = self
            .bytes
            .iter_mut()
            .skip(start)
            .zip(bytes)
            .map(|(d, s)| d.write(*s))
            .count();
        self.len = u8::try_from(start + copied).unwrap_or(u8::MAX);
    }
}
