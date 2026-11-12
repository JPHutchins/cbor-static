#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: u8,
    pub b${i}: i64,
    pub c${i}: Option<u32>,
}
