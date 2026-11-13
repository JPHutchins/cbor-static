#[derive(cbor_static::Cbor)]
#[cbor(array)]
pub struct T${i} {
    pub a${i}: ${f0},
    pub b${i}: ${f1},
    pub c${i}: ${f2},
}
