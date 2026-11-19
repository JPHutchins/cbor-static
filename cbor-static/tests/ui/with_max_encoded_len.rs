mod nothing {
    pub fn encode(_: &u8, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Reading {
    id: u8,
    #[cbor(with = nothing)]
    level: u8,
}

fn main() {}
