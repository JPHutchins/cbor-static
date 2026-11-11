#[derive(cbor_static::Cbor)]
#[cbor(index)]
pub enum T$i {
    A,
    B(u32),
    C { x: u32, y: u32 },
}
