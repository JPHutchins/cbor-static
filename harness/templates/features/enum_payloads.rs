#[derive(cbor_static::Cbor)]
pub enum T$i {
    B(u32),
    C { x: u32, y: u32 },
}
