#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: alloc::string::String,
    pub b${i}: alloc::string::String,
    pub c${i}: Option<u32>,
}
