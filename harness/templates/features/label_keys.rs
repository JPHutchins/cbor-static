#[derive(Clone, Copy)]
pub enum L$i {
    A = 1,
    B = -2,
    C = 3,
}

#[derive(cbor_static::Cbor)]
#[cbor(keys = L$i)]
pub struct T$i {
    pub a: u32,
    pub b: u32,
    pub c: Option<u32>,
}
