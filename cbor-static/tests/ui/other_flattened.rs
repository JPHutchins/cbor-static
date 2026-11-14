use cbor_static::Skipped;

#[derive(cbor_static::Cbor)]
struct Child {
    a: u8,
    #[cbor(other)]
    unknown: Skipped,
}

#[derive(cbor_static::Cbor)]
#[serde(deny_unknown_fields)]
struct Denying {
    b: u8,
    #[serde(flatten)]
    child: Child,
}

#[derive(cbor_static::Cbor)]
struct Twice {
    #[cbor(other)]
    unknown: Skipped,
    #[serde(flatten)]
    child: Child,
}

fn main() {
    let mut slot = core::mem::MaybeUninit::<Denying>::uninit();
    let _ = cbor_static::decode_into(b"\xa0", &mut slot);
    let mut slot = core::mem::MaybeUninit::<Twice>::uninit();
    let _ = cbor_static::decode_into(b"\xa0", &mut slot);
}
