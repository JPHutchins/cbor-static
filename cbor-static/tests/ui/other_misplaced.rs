use cbor_static::Skipped;

#[derive(cbor_static::Cbor)]
#[serde(deny_unknown_fields)]
struct Denying {
    #[cbor(other)]
    unknown: Skipped,
}

#[derive(cbor_static::Cbor)]
struct Twice {
    #[cbor(other)]
    first: Skipped,
    #[cbor(other)]
    second: Skipped,
}

#[derive(cbor_static::Cbor)]
#[cbor(array)]
struct Positional {
    #[cbor(other)]
    unknown: Skipped,
}

#[derive(cbor_static::Cbor)]
enum InVariant {
    A {
        #[cbor(other)]
        unknown: Skipped,
    },
}

#[derive(cbor_static::Cbor)]
struct Renamed {
    #[cbor(other, rename = "rest")]
    unknown: Skipped,
}

#[derive(cbor_static::Cbor)]
struct OfScalar {
    #[cbor(other)]
    unknown: u32,
}

fn main() {}
