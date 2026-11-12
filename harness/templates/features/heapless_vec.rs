#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: heapless::Vec<u32, 4, u8>,
    pub b${i}: heapless::Vec<u32, 4, u8>,
    pub c${i}: Option<u32>,
}
