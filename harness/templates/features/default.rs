pub fn five${i}() -> u32 {
    5
}

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    #[serde(default)]
    pub a${i}: u32,
    #[serde(default = "five${i}")]
    pub b${i}: u32,
    pub c${i}: Option<u32>,
}
