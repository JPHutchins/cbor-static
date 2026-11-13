#[derive(cbor_static::Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
enum Time<'a> {
    Text(cbor_static::DateTime<'a>),
    Epoch(cbor_static::Epoch<u64>),
}

fn main() {
    let mut slot = core::mem::MaybeUninit::uninit();
    assert_eq!(
        cbor_static::decode_into::<Time>(b"\xc1\x07", &mut slot).map(|time| *time),
        Ok(Time::Epoch(cbor_static::Tagged(7)))
    );
}
