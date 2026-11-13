#[derive(Clone, Copy)]
enum Label {
    Alg = 1,
    KeyOps = 4,
}

#[derive(cbor_static::Cbor)]
#[cbor(keys = Label)]
struct Key {
    alg: i32,
}

fn main() {}
