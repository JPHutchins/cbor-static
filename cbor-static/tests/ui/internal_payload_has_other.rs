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
}
