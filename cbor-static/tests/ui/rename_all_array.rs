#[derive(cbor_static::Cbor)]
#[cbor(array)]
#[serde(rename_all = "camelCase")]
struct Point {
    x_pos: u32,
}

fn main() {}
