#[derive(cbor_static::Encode)]
#[cbor(deterministic, length_first)]
struct Both {
    a: u8,
}

fn main() {}
