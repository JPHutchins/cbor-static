#[derive(Clone, Copy)]
enum Label {
    KeyOps = 4,
}

#[derive(cbor_static::Cbor)]
#[cbor(keys = Label)]
#[allow(non_snake_case)]
struct Header {
    key_ops: u32,
    keyOps: u32,
}

fn main() {}
