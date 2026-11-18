mod nothing {
    pub fn encode(_: &u8, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
struct Reading {
    id: u8,
    #[cbor(with = nothing)]
    level: u8,
}

fn main() {
    let _ = cbor_static::to_slice(&Reading { id: 1, level: 2 }, &mut [0u8; 8]);
}
