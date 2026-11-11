#[derive(cbor_static::Cbor)]
pub enum T$i {
    A,
    C { x: u32, y: u32 },
}
