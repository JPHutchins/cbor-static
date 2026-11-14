#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
    #[cbor(other)]
    pub d${i}: cbor_static::Skipped,
}
