#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(bytes)]
    pub a${i}: [u8; 4],
    #[cbor(bytes)]
    pub b${i}: [u8; 4],
    pub c${i}: Option<u32>,
}
