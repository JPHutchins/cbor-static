#[derive(cbor_static::Cbor)]
pub struct I$i {
    pub a: u32,
    pub b: u32,
}

#[derive(cbor_static::Cbor)]
pub struct T$i {
    pub a: I$i,
    pub c: Option<u32>,
}
