#[derive(cbor_static::Cbor)]
pub enum T${i} {
    A${i},
    C${i} { x${i}: u32, y${i}: u32 },
}
