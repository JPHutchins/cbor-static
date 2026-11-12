#[derive(cbor_static::Cbor)]
pub struct U${i};

#[derive(cbor_static::Cbor)]
pub struct N${i}(pub u32);

#[derive(cbor_static::Cbor)]
pub struct T${i} {
    pub a${i}: U${i},
    pub b${i}: N${i},
    pub c${i}: Option<u32>,
}
