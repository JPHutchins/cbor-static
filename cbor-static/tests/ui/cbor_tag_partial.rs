#[derive(cbor_static::Cbor)]
enum Cose {
    #[cbor(cbor_tag = 18)]
    Sign1(u32),
    Mac0(u32),
}

fn main() {}
