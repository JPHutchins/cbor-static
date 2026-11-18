#[derive(cbor_static::Cbor)]
pub struct T${i}<'a> {
    #[cbor(max_len = 4)]
    pub a${i}: &'a str,
    #[cbor(max_len = 4)]
    pub b${i}: &'a [u8],
}
