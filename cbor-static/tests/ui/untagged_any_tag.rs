#[derive(cbor_static::Cbor)]
#[serde(untagged)]
enum Number {
    Big(u128),
    Epoch(cbor_static::Epoch<u64>),
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    let _ = cbor_static::decode_into::<Number>(b"\x01", &mut slot);
}
