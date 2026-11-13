#[derive(cbor_static::Cbor)]
enum Cose {
    #[cbor(cbor_tag = 18)]
    Sign1(u32),
    #[cbor(cbor_tag = 17)]
    Empty,
}

fn main() {}
