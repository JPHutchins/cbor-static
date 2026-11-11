#[derive(Clone, Copy)]
pub enum L${i} {
    A${i} = 1${i},
    B${i} = -2${i},
    C${i} = 3${i},
}

#[derive(cbor_static::Cbor)]
#[cbor(keys = L${i})]
pub struct T${i} {
    pub a${i}: u32,
    pub b${i}: u32,
    pub c${i}: Option<u32>,
}
