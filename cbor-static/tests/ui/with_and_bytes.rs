mod nothing {
    pub fn encode(_: &[u8; 2], _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
struct Reading {
    #[cbor(with = nothing, bytes)]
    raw: [u8; 2],
}

fn main() {}
