mod nothing {
    pub fn encode(_: &u8, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
struct Inner {
    #[cbor(with = nothing)]
    level: u8,
}

#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
struct Reading {
    id: u8,
    inner: Inner,
}

fn main() {
    let _ = cbor_static::to_slice(&Reading { id: 1, inner: Inner { level: 2 } }, &mut [0u8; 8]);
}
