#[derive(cbor_static::Cbor)]
#[cbor(offset_of_enum)]
enum Glued {
    One(u32),
}

#[derive(cbor_static::Cbor)]
#[repr(u8)]
#[cbor(offset_of_enum)]
enum Explicit {
    One(u32) = 3,
}

fn main() {}
