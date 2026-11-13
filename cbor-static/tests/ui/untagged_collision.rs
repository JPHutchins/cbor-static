#[derive(cbor_static::Cbor)]
#[serde(untagged)]
enum Ambiguous {
    Small(u32),
    Large(u64),
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    let _ = cbor_static::decode_into::<Ambiguous>(b"\x01", &mut slot);
}
