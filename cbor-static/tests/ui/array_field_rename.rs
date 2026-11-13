#[derive(cbor_static::Cbor)]
#[cbor(array)]
struct Point {
    #[serde(rename = "x")]
    x: u32,
}

fn main() {}
