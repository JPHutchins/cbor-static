#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: f16,
    pub b${i}: f16,
    pub c${i}: Option<u32>,
}
