#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: cbor_static::F16,
    pub b${i}: cbor_static::F16,
    pub c${i}: Option<u32>,
}
