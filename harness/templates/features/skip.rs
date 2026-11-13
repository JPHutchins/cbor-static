#[derive(Default)]
pub struct Cache${i}(pub u32);

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
    #[serde(skip)]
    pub d${i}: Cache${i},
}
