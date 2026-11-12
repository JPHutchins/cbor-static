#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(bytes)]
    pub a${i}: heapless::Vec<u8, 8, u8>,
    #[cbor(bytes)]
    pub b${i}: heapless::Vec<u8, 8, u8>,
    pub c${i}: Option<u32>,
}
