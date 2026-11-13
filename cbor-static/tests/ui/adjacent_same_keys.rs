#[derive(cbor_static::Cbor)]
#[serde(tag = "t", content = "t")]
enum Message {
    Data(u32),
}

fn main() {}
