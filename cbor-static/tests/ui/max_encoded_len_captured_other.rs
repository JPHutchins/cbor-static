#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Header<'a> {
    id: u8,
    #[cbor(other)]
    unknown: heapless::Vec<cbor_static::RawEntry<'a>, 4>,
}

fn main() {}
