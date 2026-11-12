#[derive(cbor_static::Decode)]
pub struct T${i} {
    pub a${i}: heapless::LinearMap<u8, u32, 4>,
    pub b${i}: heapless::LinearMap<u8, u32, 4>,
    pub c${i}: Option<u32>,
}
