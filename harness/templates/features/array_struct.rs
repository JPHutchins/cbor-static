#[derive(cbor_static::Cbor)]
#[cbor(array)]
pub struct T${i} {
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
}
