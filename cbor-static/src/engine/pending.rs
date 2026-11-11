use crate::wire::{Head, Major};

/// Bytes produced for one item and not yet written: up to two heads plus a borrowed tail.
pub struct Pending {
    buf: [u8; 18],
    at: u8,
    len: u8,
    tail: *const u8,
    tail_len: usize,
}

impl Pending {
    pub(crate) const fn new() -> Self {
        Self {
            buf: [0; 18],
            at: 0,
            len: 0,
            tail: core::ptr::NonNull::dangling().as_ptr(),
            tail_len: 0,
        }
    }

    pub fn head(&mut self, major: Major, arg: u64) {
        self.bytes(Head::new(major, arg).as_bytes());
    }

    pub fn float(&mut self, (info, bits): (u8, u64)) {
        self.bytes(Head::float(info, bits).as_bytes());
    }

    pub fn bytes(&mut self, src: &[u8]) {
        let start = usize::from(self.len);
        let copied = self
            .buf
            .iter_mut()
            .skip(start)
            .zip(src)
            .map(|(d, s)| *d = *s)
            .count();
        self.len = u8::try_from(start + copied).unwrap_or(u8::MAX);
    }

    pub(crate) fn tail(&mut self, src: &[u8]) {
        self.tail = src.as_ptr();
        self.tail_len = src.len();
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.at == self.len && self.tail_len == 0
    }

    fn remaining(&self) -> usize {
        usize::from(self.len - self.at) + self.tail_len
    }

    /// Moves as much as fits into `out`; `oneshot` builds move all or nothing.
    #[inline(never)]
    pub(crate) fn drain(&mut self, out: &mut [u8]) -> usize {
        if cfg!(cbor_static_oneshot) && out.len() < self.remaining() {
            return 0;
        }
        let head = self
            .buf
            .get(usize::from(self.at)..usize::from(self.len))
            .unwrap_or(&[]);
        let head_written = out.iter_mut().zip(head).map(|(d, s)| *d = *s).count();
        self.at += u8::try_from(head_written).unwrap_or(0);
        let tail = unsafe { core::slice::from_raw_parts(self.tail, self.tail_len) };
        let tail_written = out.get_mut(head_written..).map_or(0, |rest| {
            rest.iter_mut().zip(tail).map(|(d, s)| *d = *s).count()
        });
        self.tail = self.tail.wrapping_add(tail_written);
        self.tail_len -= tail_written;
        if self.is_empty() {
            self.at = 0;
            self.len = 0;
        }
        head_written + tail_written
    }
}
