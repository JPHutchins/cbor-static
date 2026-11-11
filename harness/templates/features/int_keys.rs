#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(key = 1${i})]
    pub a${i}: u32,
    #[cbor(key = -2${i})]
    pub b${i}: u32,
    #[cbor(key = 3${i})]
    pub c${i}: Option<u32>,
}
