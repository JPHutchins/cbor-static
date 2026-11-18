#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Frame {
    id: u8,
    #[cbor(bytes)]
    payload: Vec<u8>,
}

fn main() {}
