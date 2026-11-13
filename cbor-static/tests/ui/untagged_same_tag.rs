#[derive(cbor_static::Cbor)]
#[serde(untagged)]
enum Stamp {
    Seconds(cbor_static::Epoch<u64>),
    Fraction(cbor_static::Epoch<f64>),
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    let _ = cbor_static::decode_into::<Stamp>(b"\xc1\x01", &mut slot);
}
