#[derive(cbor_static::Cbor)]
#[serde(rename_all = "kebab-case")]
struct Header {
    a_b: u32,
    #[serde(rename = "a-b")]
    c: u32,
}

fn main() {}
