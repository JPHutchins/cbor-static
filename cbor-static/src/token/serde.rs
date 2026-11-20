#![forbid(unsafe_code)]
//! What the serde layer reads beside the tokens themselves (feature `serde`).

use super::Tokens;
use crate::error::Error;

impl<'de> Tokens<'de> {
    pub(crate) const fn offset(&self) -> usize {
        self.input.at
    }

    pub(crate) fn rest(&self) -> &'de [u8] {
        self.input.rest()
    }

    /// The offset just past the head that starts at `at`.
    pub(crate) fn head_end(&self, at: usize) -> usize {
        use crate::wire::header_len;

        at + self
            .input
            .span(at, 1)
            .and_then(|initial| initial.first())
            .and_then(|&initial| header_len(initial).ok())
            .unwrap_or(0)
    }

    pub(crate) fn span(&self, at: usize, end: usize) -> &'de [u8] {
        self.input
            .span(at, end.saturating_sub(at))
            .unwrap_or_default()
    }

    /// The offset just past the payload of the string whose head starts at `at`.
    pub(crate) fn payload_end(&self, at: usize) -> usize {
        use crate::wire::parse;

        let head_end = self.head_end(at);
        self.input
            .span(at, head_end - at)
            .and_then(|head| parse(head).ok())
            .and_then(|head| usize::try_from(head.arg).ok())
            .map_or(head_end, |len| head_end + len)
    }

    /// The initial byte of the next head, which stays unread.
    pub(crate) fn initial(&self) -> Option<u8> {
        self.input.rest().first().copied()
    }

    /// Moves past the next item, checked for well-formedness (D-12) with a skip state of its own
    /// (D-14a, D-14b), so nesting costs no stack. On failure the tokens stay where they were, and
    /// the error is where the native decoder's skip reports it.
    #[inline(never)]
    pub(crate) fn skip_item(&mut self) -> Result<(), Error> {
        use super::WHOLE_ITEM;
        use crate::engine::Skip;
        use crate::error::ErrorKind;
        use crate::wire::length;

        let mut outer = self.skip;
        outer
            .on_header(WHOLE_ITEM)
            .map_err(|kind| Error::new(kind, 0, self.input.at))?;
        let mut input = self.input.clone();
        let mut item = Skip::started(1);
        while !item.done() {
            match length(item.payload_left()) {
                Ok(0) => input
                    .take_head()
                    .and_then(|head| head.ok_or(ErrorKind::UnexpectedEnd))
                    .and_then(|head| item.on_header(head)),
                Ok(wanted) => input
                    .take_exact(wanted)
                    .ok_or(ErrorKind::UnexpectedEnd)
                    .and_then(|payload| item.payload(payload)),
                Err(kind) => Err(kind),
            }
            .map_err(|kind| match kind {
                ErrorKind::UnexpectedEnd => Error::new(kind, 0, input.at + input.rest().len()),
                _ => Error::new(kind, 0, input.at),
            })?;
        }
        self.input = input;
        self.skip = outer;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::error::ErrorKind;
    use crate::token::{Token, Tokens};

    #[test]
    fn skipping_moves_past_exactly_one_item() {
        let mut tokens = Tokens::new(b"\x82\x9f\x01\xff\xbf\x61a\x5f\x41\x01\xff\xff\x07");
        assert_eq!(tokens.skip_item(), Ok(()));
        assert_eq!(tokens.offset(), 12);
        assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(7))));
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn skipping_inside_an_array_leaves_its_count_owed() {
        let mut tokens = Tokens::new(b"\x82\x81\x01\x02");
        assert_eq!(tokens.next(), Some(Ok(Token::Array(Some(2)))));
        assert_eq!(tokens.skip_item(), Ok(()));
        assert_eq!(tokens.next(), Some(Ok(Token::Unsigned(2))));
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn a_failed_skip_reports_past_what_failed_and_moves_nothing() {
        let cases: [(&[u8], ErrorKind, u32); 6] = [
            (b"\x82\x01", ErrorKind::UnexpectedEnd, 2),
            (b"\x82\x01\x62a", ErrorKind::UnexpectedEnd, 4),
            (b"\x9f\x82\x01\xff\x02", ErrorKind::NotWellFormed, 4),
            (b"\xbf\x00\xff", ErrorKind::NotWellFormed, 3),
            (b"\x81\x62\xc3\x28", ErrorKind::InvalidUtf8, 4),
            (b"\x81\x1c", ErrorKind::NotWellFormed, 1),
        ];
        for (bytes, kind, offset) in cases {
            let mut tokens = Tokens::new(bytes);
            assert_eq!(
                tokens.skip_item().map_err(|e| (e.kind, e.offset)),
                Err((kind, offset)),
                "{bytes:02x?}"
            );
            assert_eq!(tokens.offset(), 0);
        }
    }

    #[test]
    fn no_item_begins_inside_an_indefinite_string() {
        let mut tokens = Tokens::new(b"\x5f\x41\x01\xff");
        assert_eq!(tokens.next(), Some(Ok(Token::BytesIndefinite)));
        assert_eq!(
            tokens.skip_item().map_err(|e| (e.kind, e.offset)),
            Err((ErrorKind::NotWellFormed, 1))
        );
    }

    #[test]
    fn the_initial_byte_stays_unread() {
        let mut tokens = Tokens::new(b"\xf6");
        assert_eq!(tokens.initial(), Some(0xf6));
        assert_eq!(tokens.next(), Some(Ok(Token::Null)));
        assert_eq!(tokens.initial(), None);
    }
}
