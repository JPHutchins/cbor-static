#[derive(cbor_static::Cbor)]
struct Payload {
    t: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(tag = "t")]
enum Event {
    Data(Payload),
}

fn main() {
    let _ = cbor_static::encode_into(&Event::Data(Payload { t: 1 }), &mut [0u8; 16]);
}
