#![forbid(unsafe_code)]
//! Lengths written when a container or text closes: an unknown one into the one-byte head reserved
//! for it, moving what follows when the count needs a longer head (#6 Q6), and a declared one
//! checked against the items written.

use super::Serializer;
use crate::ErrorKind;
use crate::serde::Error;
use crate::wire::{Head, Major};

/// An array or map a [`Serializer`] opened: where its head is, and the items declared and written.
#[derive(Clone, Copy, Debug)]
pub(super) struct Open {
    start: usize,
    major: Major,
    declared: Option<usize>,
    count: usize,
}

impl Serializer<'_> {
    /// Opens an array or map of `len` items, or of a count written when it closes.
    #[inline(never)]
    pub(super) fn open(&mut self, major: Major, len: Option<usize>) -> Result<Open, Error> {
        let open = Open {
            start: self.at,
            major,
            declared: len,
            count: 0,
        };
        self.head(major, open.declared.unwrap_or(0) as u64)?;
        Ok(open)
    }

    /// Counts the next item of `open`, which must be one it declared.
    #[inline(never)]
    pub(super) fn counted(&self, open: &mut Open) -> Result<(), Error> {
        match open.declared {
            Some(declared) if open.count == declared => {
                Err(Error::new(ErrorKind::NotWellFormed, self.at))
            }
            Some(_) | None => {
                open.count += 1;
                Ok(())
            }
        }
    }

    #[inline(never)]
    pub(super) fn named(&mut self, open: &mut Open, name: &str) -> Result<(), Error> {
        self.counted(open)?;
        self.text(name)
    }

    /// Closes a sequence or map, whose declared length must be the count written, or whose head
    /// takes it.
    #[inline(never)]
    pub(super) fn close(&mut self, open: Open) -> Result<(), Error> {
        match open.declared {
            None => self.patch(open.start, Head::new(open.major, open.count as u64)),
            Some(_) => self.close_declared(open),
        }
    }

    #[inline(never)]
    pub(super) fn close_declared(&self, open: Open) -> Result<(), Error> {
        match open.declared {
            Some(declared) if declared == open.count => Ok(()),
            Some(_) | None => Err(Error::new(ErrorKind::NotWellFormed, self.at)),
        }
    }

    /// Text of `args`, as [`core::fmt::Display`] writes it.
    #[inline(never)]
    pub(super) fn formatted(&mut self, args: core::fmt::Arguments<'_>) -> Result<(), Error> {
        let start = self.at;
        self.head(Major::Text, 0)?;
        let mut text = Text {
            serializer: self,
            stopped: Ok(()),
        };
        let shown = core::fmt::write(&mut text, args);
        let Text {
            serializer,
            stopped,
        } = text;
        stopped?;
        shown.map_err(|_| Error::new(ErrorKind::Custom, serializer.at))?;
        serializer.patch(
            start,
            Head::new(Major::Text, (serializer.at - start - 1) as u64),
        )
    }

    /// Writes `head` over the byte reserved at `start`, moving what follows right to make room.
    /// When the buffer cannot hold the move, the error is at its end, where every overflow is.
    #[inline(never)]
    fn patch(&mut self, start: usize, head: Head) -> Result<(), Error> {
        use super::Sink;

        let head = head.as_bytes();
        let grown = head.len().saturating_sub(1);
        match &mut self.sink {
            #[cfg(feature = "alloc")]
            Sink::Measure => {}
            Sink::Write(out) => {
                let end = out.len();
                let moved = out
                    .get_mut(start..self.at + grown)
                    .ok_or(Error::new(ErrorKind::BufferTooSmall, end))?;
                if let Some(contents) = moved.len().checked_sub(grown) {
                    moved.copy_within(..contents, grown);
                }
                moved
                    .iter_mut()
                    .zip(head)
                    .for_each(|(slot, byte)| *slot = *byte);
            }
        }
        self.at += grown;
        Ok(())
    }
}

/// [`Serializer::formatted`]'s text as [`core::fmt`] hands it over, and the error that stopped it.
struct Text<'s, 'a> {
    serializer: &'s mut Serializer<'a>,
    stopped: Result<(), Error>,
}

impl core::fmt::Write for Text<'_, '_> {
    fn write_str(&mut self, piece: &str) -> core::fmt::Result {
        self.stopped = self
            .stopped
            .and_then(|()| self.serializer.put(&[], piece.as_bytes()));
        self.stopped.map_err(|_| core::fmt::Error)
    }
}
