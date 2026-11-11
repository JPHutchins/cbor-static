#[derive(cbor_static::Cbor)]
pub enum T$i {
    #[cbor(cbor_tag = 100)]
    B(u32),
    #[cbor(cbor_tag = 101)]
    C { x: u32, y: u32 },
}
