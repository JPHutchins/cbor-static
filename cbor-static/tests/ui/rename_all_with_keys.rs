#[derive(Clone, Copy)]
enum Label {
    KeyOps = 4,
}

#[derive(cbor_static::Cbor)]
#[cbor(keys = Label)]
#[serde(rename_all = "camelCase")]
struct Header {
    key_ops: u32,
}

fn main() {}
