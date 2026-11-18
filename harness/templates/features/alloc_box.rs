#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: alloc::boxed::Box<u32>,
    pub b${i}: alloc::boxed::Box<u32>,
    pub c${i}: Option<u32>,
}
