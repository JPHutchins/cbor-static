#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
struct Sorted<'a> {
    id: u8,
    #[cbor(other)]
    rest: heapless::Vec<cbor_static::RawEntry<'a>, 2>,
}

fn main() {
    let _ = cbor_static::encode_into(&Sorted { id: 1, rest: heapless::Vec::new() }, &mut [0u8; 16]);
}
