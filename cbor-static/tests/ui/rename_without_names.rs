#[derive(cbor_static::Cbor)]
#[serde(rename_all = "snake_case")]
enum Tagged {
    #[cbor(cbor_tag = 18)]
    SignOne(u32),
}

#[derive(cbor_static::Cbor)]
#[serde(untagged, rename_all = "camelCase")]
enum Untagged {
    Number(u64),
    Text(bool),
}

#[derive(cbor_static::Cbor)]
#[cbor(index)]
enum Indexed {
    Ping,
    #[serde(rename = "set")]
    Set(u32),
}

#[derive(cbor_static::Cbor)]
enum TaggedRenamed {
    #[cbor(cbor_tag = 18)]
    #[serde(rename = "sign1")]
    SignOne(u32),
}

fn main() {}
