#[derive(cbor_static::Cbor)]
#[serde(tag = "type")]
enum Event {
    Move {
        x: u32,
        #[serde(rename = "type")]
        kind: u32,
    },
}

fn main() {}
