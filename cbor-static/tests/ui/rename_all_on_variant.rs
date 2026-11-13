#[derive(cbor_static::Cbor)]
enum Event {
    #[serde(rename_all = "camelCase")]
    Move { x_pos: u32 },
}

fn main() {}
