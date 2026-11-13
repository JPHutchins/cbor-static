#[derive(cbor_static::Cbor)]
#[cbor(array)]
struct Point {
    x: u32,
    y: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(tag = "type")]
enum Event<'a> {
    Count(u32),
    At(Point),
    Text(cbor_static::Chunks<'a, str>),
}

fn main() {}
