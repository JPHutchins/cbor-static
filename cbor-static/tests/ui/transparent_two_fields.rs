#[derive(cbor_static::Cbor)]
#[serde(transparent)]
struct Pair {
    a: u32,
    b: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(transparent)]
struct Tuple(u32, u32);

#[derive(cbor_static::Cbor)]
#[serde(transparent, deny_unknown_fields)]
struct Strict {
    a: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(transparent)]
enum Choice {
    A(u32),
}

fn main() {}
