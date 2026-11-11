#[derive(cbor_static::Cbor)]
pub enum T${i} {
    #[cbor(cbor_tag = 1${i}0)]
    B${i}(u32),
    #[cbor(cbor_tag = 1${i}1)]
    C${i} { x${i}: u32, y${i}: u32 },
}
