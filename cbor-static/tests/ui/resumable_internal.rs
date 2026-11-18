#[derive(cbor_static::Decode)]
#[serde(tag = "type")]
enum Event {
    Start,
}

fn main() {
    cbor_static::__private::resumable::<Event>();
}
