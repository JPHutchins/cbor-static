#[derive(cbor_static::Cbor)]
#[cbor(deny_unknown_fields)]
pub struct T$i {
    #[cbor(key = 1)]
    pub a: u32,
    #[cbor(key = -2)]
    pub b: u32,
    #[cbor(key = 3)]
    pub c: Option<u32>,
}
