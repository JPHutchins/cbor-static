#[derive(cbor_static::Cbor)]
pub struct T${i}<'a> {
    pub a${i}: &'a str,
    pub b${i}: &'a [u8],
}
