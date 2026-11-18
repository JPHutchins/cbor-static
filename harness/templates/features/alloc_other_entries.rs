#[derive(cbor_static::Cbor)]
pub struct T${i}<'a> {
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
    #[cbor(other)]
    pub d${i}: alloc::vec::Vec<cbor_static::RawEntry<'a>>,
}
