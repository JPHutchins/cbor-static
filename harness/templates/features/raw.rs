#[derive(cbor_static::Cbor)]
pub struct T${i}<'a> {
    pub a${i}: cbor_static::RawCbor<'a>,
    pub b${i}: cbor_static::RawCbor<'a>,
    pub c${i}: Option<u32>,
}
