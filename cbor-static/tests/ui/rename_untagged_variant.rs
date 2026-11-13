#[derive(cbor_static::Cbor)]
#[serde(untagged)]
enum Id {
    #[serde(rename = "number")]
    Number(u64),
    Flag(bool),
}

fn main() {}
