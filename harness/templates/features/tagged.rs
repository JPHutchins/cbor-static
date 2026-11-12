#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: cbor_static::Tagged<1, u32>,
    pub b${i}: cbor_static::Tagged<1, u32>,
    pub c${i}: Option<u32>,
}
