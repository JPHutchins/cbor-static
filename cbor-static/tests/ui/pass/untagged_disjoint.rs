#[derive(cbor_static::Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
enum Id<'a> {
    Number(u64),
    Name(&'a str),
    Missing,
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    assert_eq!(
        cbor_static::decode_into::<Id>(b"\xf6", &mut slot).map(|id| *id),
        Ok(Id::Missing)
    );
}
