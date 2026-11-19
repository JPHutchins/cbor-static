//! The part of a `with` field's one item that lands in a [`super::TokenWriter`]'s buffer when the
//! field's module runs again for it (#5 Q19). A writer reaches it only through [`WindowOps`], which
//! only a `with` field's ops make (D-22).

use super::writer::encoding;
use super::{Token, TokenWriter};
use crate::error::{Error, ErrorKind};

/// The bytes from `from` on, and the items still owed, starting at one.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    from: usize,
    owed: u64,
    pub(super) ops: &'static WindowOps,
}

/// How a writer spends and fills a window.
#[derive(Debug)]
pub struct WindowOps {
    /// Writes `token` as part of the window's item.
    pub(super) write: for<'w, 'a> fn(
        &'w mut TokenWriter<'a>,
        Token<'_>,
    ) -> Result<&'w mut TokenWriter<'a>, Error>,
    /// Spends one owed item on `token`, which owes the items it heads; [`WHOLE_ITEM`] owes none.
    pub(super) owe: fn(&mut Window, Token<'_>) -> Result<(), ErrorKind>,
    /// Copies those of `bytes`, written at offset `at` of the item, that the window covers.
    pub(super) put: fn(&Window, &mut [u8], usize, &[u8]),
}

/// A whole item written at once, as a typed value is: it heads nothing.
pub const WHOLE_ITEM: Token<'static> = Token::Null;

static WINDOW: WindowOps = WindowOps { write, owe, put };

impl Window {
    pub(crate) const fn new(from: usize) -> Self {
        Self {
            from,
            owed: 1,
            ops: &WINDOW,
        }
    }

    /// Whether exactly one item was written, and finished.
    pub(super) const fn done(&self) -> bool {
        self.owed == 0
    }
}

fn write<'w, 'a>(
    writer: &'w mut TokenWriter<'a>,
    token: Token<'_>,
) -> Result<&'w mut TokenWriter<'a>, Error> {
    let at = writer.at;
    let (head, payload) = encoding(token).map_err(|kind| Error::new(kind, 0, at))?;
    let window = writer
        .item
        .as_deref_mut()
        .ok_or(Error::new(ErrorKind::Unsupported, 0, at))?;
    owe(window, token).map_err(|kind| Error::new(kind, 0, at))?;
    put(window, writer.out, at, head.as_bytes());
    put(window, writer.out, at + head.as_bytes().len(), payload);
    writer.at += head.as_bytes().len() + payload.len();
    Ok(writer)
}

fn owe(window: &mut Window, token: Token<'_>) -> Result<(), ErrorKind> {
    window.owed = window
        .owed
        .checked_sub(1)
        .map(|left| left.saturating_add(owed_within(token)))
        .ok_or(ErrorKind::NotWellFormed)?;
    Ok(())
}

fn put(window: &Window, out: &mut [u8], at: usize, bytes: &[u8]) {
    out.iter_mut()
        .skip(at.saturating_sub(window.from))
        .zip(bytes.iter().skip(window.from.saturating_sub(at)))
        .for_each(|(slot, byte)| *slot = *byte);
}

/// The items a token's head owes after it: an array's elements, a map's keys and values, a tag's
/// content.
const fn owed_within(token: Token<'_>) -> u64 {
    match token {
        Token::Array(Some(len)) => len,
        Token::Map(Some(len)) => len.saturating_mul(2),
        Token::Tag(_) => 1,
        Token::Unsigned(_)
        | Token::Negative(_)
        | Token::Bytes(_)
        | Token::Text(_)
        | Token::BytesIndefinite
        | Token::TextIndefinite
        | Token::Array(None)
        | Token::Map(None)
        | Token::Simple(_)
        | Token::Bool(_)
        | Token::Null
        | Token::Undefined
        | Token::Float(_)
        | Token::Break => 0,
    }
}
