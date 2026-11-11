#[derive(cbor_static::Cbor)]
#[serde(untagged)]
pub enum T$i {
    A,
    B(u32),
    C { x: u32, y: u32 },
}
