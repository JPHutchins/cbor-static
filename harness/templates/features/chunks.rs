#[derive(cbor_static::Cbor)]
pub struct T${i}<'a> {
    pub a${i}: cbor_static::Chunks<'a, str>,
    pub b${i}: cbor_static::Chunks<'a, [u8]>,
}
