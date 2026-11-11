#[derive(cbor_static::Cbor)]
#[cbor(deny_unknown_fields)]
pub struct T$i {
    pub a: u32,
    pub b: u32,
    pub c: Option<u32>,
}
