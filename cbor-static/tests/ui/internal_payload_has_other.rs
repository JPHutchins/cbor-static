#[derive(cbor_static::Cbor)]
struct Payload {
    a: u32,
    #[cbor(other)]
    unknown: cbor_static::Skipped,
}

#[derive(cbor_static::Cbor)]
#[serde(tag = "t")]
enum Event {
    Data(Payload),
}

fn main() {
    let _ = cbor_static::encode_into(&Event::Data(Payload { a: 1, unknown: Default::default() }), &mut [0u8; 16]);
    let mut slot = core::mem::MaybeUninit::uninit();
    let _ = cbor_static::decode_into::<Event>(b"\xa2\x61t\x64Data\x61a\x01", &mut slot);
}
