#[derive(cbor_static::Cbor)]
struct Reading {
    id: u32,
    #[serde(skip_serializing)]
    cache: u32,
}

fn main() {}
