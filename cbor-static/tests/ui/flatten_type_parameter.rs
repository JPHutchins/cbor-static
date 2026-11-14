#[derive(cbor_static::Cbor)]
struct Envelope<T> {
    id: u8,
    #[cbor(flatten)]
    body: T,
}

fn main() {}
