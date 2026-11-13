#[derive(cbor_static::Cbor)]
struct Reading {
    id: u32,
    #[serde(skip, rename = "c")]
    cache: u32,
}

fn main() {}
