#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: char,
    pub b${i}: char,
    pub c${i}: Option<u32>,
}
