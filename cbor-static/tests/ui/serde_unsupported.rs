#[derive(cbor_static::Cbor)]
struct Aliased {
    #[serde(alias = "identifier")]
    id: u32,
}

#[derive(cbor_static::Cbor)]
struct With {
    #[serde(with = "module")]
    id: u32,
}

#[derive(cbor_static::Cbor)]
struct Serialize {
    #[serde(serialize_with = "module::serialize")]
    id: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(from = "u32")]
struct From {
    id: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(into = "u32")]
struct Into {
    id: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(try_from = "u32")]
struct TryFrom {
    id: u32,
}

#[derive(cbor_static::Cbor)]
#[serde(bound = "")]
struct Bound {
    id: u32,
}

#[derive(cbor_static::Cbor)]
enum Other {
    Known(u32),
    #[serde(other)]
    Unknown,
}

fn main() {}
