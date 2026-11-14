#[derive(cbor_static::Encode)]
struct Declared {
    a: u8,
}

#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
struct Sorted {
    b: u8,
    #[serde(flatten)]
    declared: Declared,
}

fn main() {
    let _ = cbor_static::encode_into(&Sorted { b: 1, declared: Declared { a: 2 } }, &mut [0u8; 16]);
}
