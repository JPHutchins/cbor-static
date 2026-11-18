#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(bytes)]
    pub a${i}: alloc::vec::Vec<u8>,
    #[cbor(bytes)]
    pub b${i}: alloc::vec::Vec<u8>,
    pub c${i}: Option<u32>,
}
