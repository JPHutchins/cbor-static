mod as_map {
    pub fn encode(_: &u8, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
#[serde(tag = "type")]
enum Event {
    Level(#[cbor(with = as_map)] u8),
}

fn main() {}
