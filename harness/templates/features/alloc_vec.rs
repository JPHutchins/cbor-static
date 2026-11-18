#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: alloc::vec::Vec<u32>,
    pub b${i}: alloc::vec::Vec<u32>,
    pub c${i}: Option<u32>,
}
