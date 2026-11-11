#[derive(cbor_static::Cbor)]
#[cbor(array)]
pub struct T$i {
    pub a: u32,
    pub b: u32,
    pub c: Option<u32>,
}
