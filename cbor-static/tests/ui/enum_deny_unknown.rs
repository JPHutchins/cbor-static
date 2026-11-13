#[derive(cbor_static::Cbor)]
#[serde(tag = "type", deny_unknown_fields)]
enum Event {
    Move { x: i32 },
}

fn main() {}
