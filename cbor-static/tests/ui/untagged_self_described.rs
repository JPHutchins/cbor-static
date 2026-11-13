#[derive(cbor_static::Cbor)]
#[serde(untagged)]
enum Stamp {
    Described(cbor_static::SelfDescribed<bool>),
    Epoch(cbor_static::Epoch<u64>),
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    let _ = cbor_static::decode_into::<Stamp>(b"\xf5", &mut slot);
}
