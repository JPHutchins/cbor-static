#[derive(cbor_static::Cbor)]
struct Key {
    #[cbor(max_len = 2)]
    id: [u8; 4],
}

#[derive(cbor_static::Encode)]
struct Count {
    #[cbor(max_len = 2)]
    n: u32,
}

fn main() {}
