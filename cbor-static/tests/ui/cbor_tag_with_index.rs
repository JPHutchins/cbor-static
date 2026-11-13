#[derive(cbor_static::Cbor)]
#[cbor(index)]
enum Cose {
    #[cbor(cbor_tag = 18)]
    Sign1(u32),
}

fn main() {}
