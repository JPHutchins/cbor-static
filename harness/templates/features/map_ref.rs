#[derive(cbor_static::Cbor)]
pub struct T${i}<'a> {
    pub a${i}: cbor_static::MapRef<'a, u8, u32>,
    pub b${i}: cbor_static::MapRef<'a, u8, u32>,
    pub c${i}: Option<u32>,
}
