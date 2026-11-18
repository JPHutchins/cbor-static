#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Reading {
    id: u32,
}

fn main() {
    let _ = cbor_static::encode_into(&Reading { id: 1 }, &mut [0u8; 8]);
}
