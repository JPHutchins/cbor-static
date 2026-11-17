#[derive(cbor_static::Cbor)]
enum Command {
    Seek {
        #[cbor(key = 1)]
        to: u32,
        #[cbor(key = 1)]
        whence: u32,
    },
}

fn main() {}
