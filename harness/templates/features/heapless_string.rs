#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: heapless::String<8, u8>,
    pub b${i}: heapless::String<8, u8>,
    pub c${i}: Option<u32>,
}
