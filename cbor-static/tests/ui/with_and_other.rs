mod nothing {
    pub fn encode(_: &cbor_static::Skipped, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
struct Reading {
    level: u8,
    #[cbor(other, with = nothing)]
    unknown: cbor_static::Skipped,
}

fn main() {}
