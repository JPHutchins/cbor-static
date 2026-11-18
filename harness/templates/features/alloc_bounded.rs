#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: cbor_static::Bounded<alloc::string::String, 8>,
    pub b${i}: cbor_static::Bounded<alloc::string::String, 8>,
    pub c${i}: Option<u32>,
}
