#[derive(cbor_static::Cbor)]
struct Inner {
    x: u32,
}

#[derive(cbor_static::Cbor)]
struct Config {
    #[serde(default)]
    inner: Inner,
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    let _ = cbor_static::decode_into::<Config>(b"\xa0", &mut slot);
}
