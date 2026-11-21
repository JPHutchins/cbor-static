//! RFC 8392 A.2's keys. Each writes its key type's parameters first, so the bytes round-trip
//! through the order-preserving map, and `COSE_Key` is compared with it as data.
//!
//! A.2.2's hex and its diagnostic notation disagree, and the hex is the vector. Two errata, both
//! Reported:
//! - 7982 (<https://www.rfc-editor.org/errata/eid7982>) corrects the notation's
//!   `/ kid /  4: h'53796d6d6574726963323536' / 'Symmetric256' /,` to `/ kid /  2: …`: "The hex
//!   above the diagnostic notation encodes for index 2 before the 'Symmetric256' value."
//! - 8655 (<https://www.rfc-editor.org/errata/eid8655>) would change the hex's last byte: "The key
//!   advertises algorithm number 10 (AES-CCM-16-64-128) instead of 4 (HMAC 256/64). The last byte
//!   needs to change from 0a to 04 to match the "/ alg /  3: 4 / HMAC 256/64 /" CBOR diagnostic."

use std::collections::BTreeMap;

use cbor_static_conformance::cose::{Key, Label};

use crate::common::{encode, reencode};

/// The key's entries as written, each label and value as bytes.
fn written(bytes: &[u8]) -> BTreeMap<Vec<u8>, Vec<u8>> {
    use cbor_static::RawCbor;
    use heapless::LinearMap;

    cbor_static::from_slice::<LinearMap<Label, RawCbor, 8>>(bytes)
        .expect("a map")
        .iter()
        .map(|(label, value)| (encode(label).expect("encodes"), value.as_bytes().to_vec()))
        .collect()
}

/// The typed key's entries, each label and value as bytes.
fn typed(key: &Key) -> BTreeMap<Vec<u8>, Vec<u8>> {
    [
        (1, Some(encode(&key.kty))),
        (2, key.kid.map(|kid| encode(&kid))),
        (3, key.alg.map(|alg| encode(&alg))),
        (4, key.key_ops.as_ref().map(encode)),
        (5, key.base_iv.map(|iv| encode(&iv))),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        value.map(|value| {
            (
                encode(&Label::Int(label)).expect("encodes"),
                value.expect("encodes"),
            )
        })
    })
    .chain(key.parameters.iter().map(|entry| {
        (
            entry.key.as_bytes().to_vec(),
            entry.value.as_bytes().to_vec(),
        )
    }))
    .collect()
}

macro_rules! keys {
    ($($example:ident),* $(,)?) => {
        $(
            mod $example {
                use super::*;

                #[test]
                fn decodes_as_written() {
                    assert_eq!(
                        cbor_static::from_slice::<Key>(rfc8392!($example)).as_ref().map(typed),
                        Ok(written(rfc8392!($example)))
                    );
                }

                #[test]
                fn the_typed_key_writes_its_named_parameters_first() {
                    assert!(matches!(
                        reencode::<Key>(rfc8392!($example)),
                        Ok(written) if written != rfc8392!($example)
                    ));
                }
            }
        )*
    };
}

keys!(appendix_a_2_1, appendix_a_2_2, appendix_a_2_3);

#[test]
fn the_256_bit_key_is_read_as_its_hex_writes_it() {
    assert_eq!(
        cbor_static::from_slice::<Key>(rfc8392!(appendix_a_2_2)).map(|key| (key.kid, key.alg)),
        Ok((Some(b"Symmetric256".as_slice()), Some(Label::Int(10))))
    );
}
