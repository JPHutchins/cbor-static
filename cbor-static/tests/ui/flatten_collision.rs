#[derive(cbor_static::Cbor)]
struct Child {
    x: u8,
}

#[derive(cbor_static::Cbor)]
struct Parent {
    x: u8,
    #[serde(flatten)]
    child: Child,
}

fn main() {
    let _ = cbor_static::encode_into(&Parent { x: 1, child: Child { x: 2 } }, &mut [0u8; 16]);
}
