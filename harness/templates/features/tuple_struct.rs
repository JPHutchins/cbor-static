#[derive(cbor_static::Cbor)]
pub struct P${i}(pub ${f0}, pub ${f1}, pub ${f2});

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: P${i},
    pub c${i}: Option<u32>,
}
