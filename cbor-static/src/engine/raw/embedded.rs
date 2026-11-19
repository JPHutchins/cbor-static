//! [`crate::Embedded`]: a value decoded from tag 24's byte string by a decoder of its own, under the
//! outer decoder's profile (#5 Q18). The item is taken whole, as a raw item is.

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use super::after_head;
use super::codec::borrowed;
use crate::engine::{DECODE_RAW as RAW, Decoder, Fed, Input, Item, drop_fn};
use crate::error::ErrorKind;
use crate::schema::{DecKind, Decode};
use crate::wire::{ENCODED_CBOR, Major, length};

/// The descriptor of an embedded `T`, which `Embedded<T>` holds alone.
pub struct EmbeddedDecoding<'de, T>(PhantomData<(&'de (), T)>);

impl<'de, T: Decode<'de>> EmbeddedDecoding<'de, T> {
    pub const DECODE: &'static DecKind = &DecKind::Raw {
        ops: &RAW,
        store: store::<T>,
        drop: drop_fn::<T>(),
    };
}

/// Decodes the byte string's contents, all of them, into the slot.
unsafe fn store<'de, T: Decode<'de>>(dst: *mut u8, item: &Item<'_>) -> Result<(), ErrorKind> {
    let contents = unsafe { borrowed(contents(item)?) };
    let mut decoder = Decoder::under(unsafe { &mut *dst.cast::<MaybeUninit<T>>() }, item.strict);
    match decoder.feed(contents).map_err(|error| error.kind)? {
        Fed::Done(used) if used == contents.len() => {
            decoder.finish().map(|_| ()).map_err(|error| error.kind)
        }
        Fed::Done(_) => Err(ErrorKind::TrailingInput),
        Fed::NeedInput => Err(ErrorKind::UnexpectedEnd),
    }
}

/// The contents of the definite byte string tag 24 wraps.
fn contents<'a>(item: &Item<'a>) -> Result<&'a [u8], ErrorKind> {
    match (item.header.major, item.header.arg) {
        (Major::Tag, ENCODED_CBOR) => {
            let mut input = Input::new(after_head(item.payload));
            let header = input.take_head()?.ok_or(ErrorKind::UnexpectedEnd)?;
            match (header.major, header.indefinite()) {
                (Major::Bytes, false) => input
                    .take_exact(length(header.arg)?)
                    .ok_or(ErrorKind::UnexpectedEnd),
                (Major::Bytes, true) => Err(ErrorKind::IndefiniteString),
                _ => Err(ErrorKind::TypeMismatch),
            }
        }
        (Major::Tag, _) => Err(ErrorKind::UnexpectedTag),
        _ => Err(ErrorKind::TypeMismatch),
    }
}
