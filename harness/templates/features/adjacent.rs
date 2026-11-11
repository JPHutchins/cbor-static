#[derive(cbor_static::Cbor)]
#[serde(tag = "t${i}", content = "c${i}")]
pub enum T${i} {
    A${i},
    B${i}(u32),
    C${i} { x${i}: u32, y${i}: u32 },
}
