mod maybe {
    pub fn encode(_: &Option<u8>, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
struct Reading {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cbor(with = maybe)]
    level: Option<u8>,
}

fn main() {}
