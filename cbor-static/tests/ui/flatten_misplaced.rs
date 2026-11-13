#[derive(cbor_static::Cbor)]
struct Child {
    x: u8,
}

#[derive(cbor_static::Cbor)]
#[cbor(array)]
struct InArray {
    #[serde(flatten)]
    child: Child,
}

#[derive(cbor_static::Cbor)]
enum InVariant {
    A {
        #[serde(flatten)]
        child: Child,
    },
}

#[derive(cbor_static::Cbor)]
struct WithRename {
    #[serde(flatten, rename = "c")]
    child: Child,
}

#[derive(cbor_static::Cbor)]
struct InTuple(#[serde(flatten)] Child, u8);

fn main() {}
