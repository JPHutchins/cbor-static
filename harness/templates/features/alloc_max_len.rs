#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(max_len = 4)]
    pub a${i}: alloc::vec::Vec<u32>,
    #[cbor(bytes, max_len = 4)]
    pub b${i}: alloc::vec::Vec<u8>,
    pub c${i}: Option<u32>,
}
