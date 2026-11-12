#[derive(cbor_static::Cbor)]
pub struct P${i}(pub u32, pub u32);

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: P${i},
    pub c${i}: Option<u32>,
}
