#[derive(cbor_static::Cbor)]
#[repr(u8)]
#[cbor(offset_of_enum)]
enum Tagged {
    One(u32),
}

fn main() {}
