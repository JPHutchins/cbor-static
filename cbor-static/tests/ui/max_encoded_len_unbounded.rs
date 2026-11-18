#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
struct Note<'a> {
    id: u8,
    text: &'a str,
}

#[derive(cbor_static::Encode)]
#[cbor(max_encoded_len)]
enum Event<'a> {
    Started,
    Logged { line: &'a [u8] },
}

fn main() {}
