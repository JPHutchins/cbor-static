#[derive(cbor_static::Cbor)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Header {
    key_ops: u32,
}

fn main() {}
