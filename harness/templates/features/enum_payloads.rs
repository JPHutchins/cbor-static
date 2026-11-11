#[derive(cbor_static::Cbor)]
pub enum T${i} {
    B${i}(u32),
    C${i} { x${i}: u32, y${i}: u32 },
}
