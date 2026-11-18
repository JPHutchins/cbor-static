mod nothing {
    pub fn encode(_: &super::Inner, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
struct Inner {
    level: u8,
}

#[derive(cbor_static::Encode)]
struct Reading {
    #[serde(flatten)]
    #[cbor(with = nothing)]
    inner: Inner,
}

fn main() {}
