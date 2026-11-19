pub mod with${i} {
    use core::mem::MaybeUninit;

    use cbor_static::{Error, TokenWriter, Tokens};

    pub fn encode(value: &u32, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.encode(value).map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<u32, Error> {
        tokens.decode_into(&mut MaybeUninit::uninit()).map(|value| *value)
    }
}

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(with = with${i})]
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
}
