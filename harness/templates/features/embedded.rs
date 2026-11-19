#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: cbor_static::Embedded<u32>,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
}
