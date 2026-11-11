#[derive(cbor_static::Cbor)]
#[serde(tag = "t${i}")]
pub enum T${i} {
    A${i},
    C${i} { x${i}: u32, y${i}: u32 },
}
