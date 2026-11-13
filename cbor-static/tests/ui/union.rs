#[derive(cbor_static::Cbor)]
union Bits {
    word: u32,
    bytes: [u8; 4],
}

fn main() {}
