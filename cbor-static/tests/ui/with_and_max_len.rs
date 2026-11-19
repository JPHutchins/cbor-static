mod nothing {
    pub fn encode(_: &&str, _: &mut cbor_static::TokenWriter<'_>) -> Result<(), cbor_static::Error> {
        Ok(())
    }
}

#[derive(cbor_static::Encode)]
struct Reading<'a> {
    #[cbor(with = nothing, max_len = 4)]
    name: &'a str,
}

fn main() {}
