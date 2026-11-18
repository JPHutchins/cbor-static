#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[cbor(max_len = 4)]
    pub a${i}: alloc::collections::BTreeMap<u8, u32>,
    #[cbor(max_len = 4)]
    pub b${i}: alloc::collections::BTreeMap<u8, u32>,
    pub c${i}: Option<u32>,
}
