#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
#[serde(tag = "type")]
enum Internal {
    Start { at: u32 },
}

#[derive(cbor_static::Encode)]
#[cbor(length_first)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Start(u32),
}

fn main() {}
