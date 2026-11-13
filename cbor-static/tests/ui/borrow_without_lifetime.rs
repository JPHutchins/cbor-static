#[derive(cbor_static::Cbor)]
struct Count {
    #[serde(borrow)]
    count: u32,
}

#[derive(cbor_static::Cbor)]
struct Named<'a> {
    #[serde(borrow = "'b")]
    name: &'a str,
}

#[derive(cbor_static::Cbor)]
struct Fixed {
    #[serde(borrow)]
    name: &'static str,
}

fn main() {}
