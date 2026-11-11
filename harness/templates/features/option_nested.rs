#[derive(cbor_static::Cbor)]
pub struct I${i} {
    pub a${i}: u32,
    pub b${i}: u32,
}

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: Option<I${i}>,
    pub c${i}: Option<u32>,
}
