#[derive(cbor_static::Cbor)]
struct Pair(u8, u8);

#[derive(cbor_static::Cbor)]
struct OfScalar {
    #[serde(flatten)]
    count: u32,
}

#[derive(cbor_static::Cbor)]
struct OfArray {
    #[serde(flatten)]
    pair: Pair,
}

fn main() {}
