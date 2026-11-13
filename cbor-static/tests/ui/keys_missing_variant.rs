#[derive(Clone, Copy)]
enum Label {
    Alg = 1,
}

#[derive(cbor_static::Cbor)]
#[cbor(keys = Label)]
struct Key {
    alg: i32,
    key_ops: u32,
}

fn main() {}
