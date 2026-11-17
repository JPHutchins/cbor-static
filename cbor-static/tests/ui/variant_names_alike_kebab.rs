#[derive(cbor_static::Cbor)]
#[serde(rename_all = "kebab-case")]
#[allow(non_camel_case_types)]
enum Mode {
    AB,
    A_b,
}

fn main() {}
