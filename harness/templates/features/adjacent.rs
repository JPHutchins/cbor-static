#[derive(cbor_static::Cbor)]
#[serde(tag = "t", content = "c")]
pub enum T$i {
    A,
    B(u32),
    C { x: u32, y: u32 },
}
