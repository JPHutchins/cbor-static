#[derive(cbor_static::Encode)]
struct Declared {
    a: u8,
}

#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
struct Bytewise {
    a: u8,
}

#[derive(cbor_static::Encode)]
#[cbor(length_first)]
struct LengthFirst {
    a: u8,
}

#[derive(cbor_static::Encode)]
#[cbor(deterministic)]
struct HoldsDeclared {
    part: [Declared; 1],
}

#[derive(cbor_static::Encode)]
#[cbor(length_first)]
struct HoldsConflict {
    pair: (Bytewise, LengthFirst),
}

fn main() {
    let _ = cbor_static::encode_into(&HoldsDeclared { part: [Declared { a: 1 }] }, &mut [0u8; 16]);
    let _ = cbor_static::encode_into(
        &HoldsConflict {
            pair: (Bytewise { a: 1 }, LengthFirst { a: 2 }),
        },
        &mut [0u8; 16],
    );
}
