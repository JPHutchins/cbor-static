#[derive(cbor_static::Decode)]
struct Reading<'a> {
    name: &'a str,
}

fn main() {
    cbor_static::__private::resumable::<Reading<'static>>();
}
