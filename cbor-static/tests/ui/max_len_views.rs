use cbor_static::{ArrayRef, Chunks, MapRef};

#[derive(cbor_static::Cbor)]
struct Views<'a> {
    #[cbor(max_len = 4)]
    text: Chunks<'a, str>,
    #[cbor(max_len = 4)]
    values: ArrayRef<'a, u32>,
    #[cbor(max_len = 4)]
    entries: MapRef<'a, u8, u32>,
}

fn main() {}
