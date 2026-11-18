pub mod with${i} {
    use cbor_static::{Error, ErrorKind, Token, TokenWriter, Tokens};

    pub fn encode(value: &u32, writer: &mut TokenWriter<'_>) -> Result<(), Error> {
        writer.write(Token::Unsigned((*value).into())).map(|_| ())
    }

    pub fn decode(tokens: &mut Tokens<'_>) -> Result<u32, Error> {
        match tokens.next() {
            Some(Ok(Token::Unsigned(value))) => {
                u32::try_from(value).map_err(|_| ErrorKind::OutOfRange.into())
            }
            Some(Err(error)) => Err(error),
            Some(Ok(_)) | None => Err(ErrorKind::TypeMismatch.into()),
        }
    }
}

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(with = with${i})]
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
}
