#[derive(cbor_static::Cbor)]
struct Name {
    #[cbor(max_len = 8)]
    name: heapless::String<4>,
}

fn main() {
    let _ = cbor_static::to_slice(&Name { name: heapless::String::new() }, &mut [0u8; 16]);
}
