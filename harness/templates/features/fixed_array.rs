#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: [u32; 2],
    pub c${i}: Option<u32>,
}
