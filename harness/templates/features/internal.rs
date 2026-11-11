#[derive(cbor_static::Cbor)]
#[serde(tag = "t")]
pub enum T$i {
    A,
    C { x: u32, y: u32 },
}
