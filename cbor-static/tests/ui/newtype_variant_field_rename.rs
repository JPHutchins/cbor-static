#[derive(cbor_static::Cbor)]
enum Frame {
    Raw(#[serde(rename = "r")] [u8; 4]),
}

fn main() {}
