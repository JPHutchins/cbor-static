#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: alloc::collections::BTreeMap<u8, u32>,
    pub b${i}: alloc::collections::BTreeMap<u8, u32>,
    pub c${i}: Option<u32>,
}
