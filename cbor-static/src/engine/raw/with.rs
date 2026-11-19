//! A value a `#[cbor(with = module)]` field's module decodes (#5 Q19), from tokens over exactly its
//! item, taken whole and checked as a raw item is, under the decoder's profile.

use core::marker::PhantomData;

use super::codec::borrowed;
use crate::engine::{DECODE_RAW as RAW, Item, drop_fn};
use crate::error::{Error, ErrorKind};
use crate::schema::DecKind;
use crate::token::Tokens;

/// A field's module, which reads its value from the tokens of exactly one item. The derive
/// implements it on a type of its own per field, whose `decode` calls the module's.
pub trait WithDecode<'de> {
    type Value;
    /// # Errors
    /// Whatever the module's `decode` returns.
    fn decode(tokens: &mut Tokens<'de>) -> Result<Self::Value, Error>;
}

/// The descriptor of a value `M` decodes from input that outlives `'de`.
pub struct WithDecoding<'de, M>(PhantomData<(&'de (), M)>);

impl<'de, M: WithDecode<'de>> WithDecoding<'de, M> {
    pub const DECODE: &'static DecKind = &DecKind::Raw {
        ops: &RAW,
        store: store::<M>,
        drop: drop_fn::<M::Value>(),
    };
}

/// Writes what the module reads from the item's tokens, once it has read every one of them.
unsafe fn store<'de, M: WithDecode<'de>>(dst: *mut u8, item: &Item<'_>) -> Result<(), ErrorKind> {
    let mut tokens = Tokens::under(unsafe { borrowed(item.payload) }, item.strict);
    let value = M::decode(&mut tokens).map_err(|error| error.kind)?;
    if tokens.next().is_some() {
        return Err(ErrorKind::TrailingInput);
    }
    unsafe { dst.cast::<M::Value>().write(value) };
    Ok(())
}
