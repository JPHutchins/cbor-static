#[derive(cbor_static::Cbor)]
#[cbor(index)]
#[serde(rename_all = "snake_case")]
enum Command {
    Ping,
    Set(u32),
}

fn main() {}
