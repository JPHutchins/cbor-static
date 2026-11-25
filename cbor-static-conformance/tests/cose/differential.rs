//! Every item the COSE types write, through ciborium, minicbor and cbor2: messages, header maps,
//! signers and recipients, keys and key sets, one case per example and part; and each intermediate
//! structure the types build, one case each.

use cbor_static::Error;
use cbor_static_conformance::cose::{
    Encrypt, Encrypt0, HeaderMap, Key, KeySet, Mac, Mac0, Message, Recipient, Sign, Sign1,
    Signature,
};

use crate::common::differential::typed::round_trip;
use crate::common::vectors::{Failure, Kind, Role, Vector, intermediate, vector};
use crate::common::{encode, reencode};
use crate::decoded::{intact, reencoded};
use crate::headers::maps;
use crate::intermediates::{NOT_ENC_STRUCTURES, structure_reencoded};
use crate::keys::{key_set, keys};

/// The message as its form reads it back: tagged, or as the bare struct the generator left.
fn message_read_back(vector: &'static Vector<Kind>) -> impl Fn(&[u8]) -> Result<Vec<u8>, Error> {
    move |bytes| match (
        vector.expect.ops().contains(&Failure::RemoveCborTag),
        vector.kind,
    ) {
        (false, _) => reencode::<Message>(bytes),
        (true, Kind::Sign) => reencode::<Sign>(bytes),
        (true, Kind::Sign1) => reencode::<Sign1>(bytes),
        (true, Kind::Encrypt) => reencode::<Encrypt>(bytes),
        (true, Kind::Encrypt0) => reencode::<Encrypt0>(bytes),
        (true, Kind::Mac) => reencode::<Mac>(bytes),
        (true, Kind::Mac0) => reencode::<Mac0>(bytes),
    }
}

fn message(name: &str) {
    let vector = vector(name);
    if intact(vector).is_some() {
        round_trip(
            &reencoded(vector).expect("re-encodes"),
            message_read_back(vector),
        );
    }
}

fn headers(name: &str) {
    for (_, map) in maps(vector(name)) {
        round_trip(&map, |bytes| reencode::<HeaderMap>(bytes));
    }
}

fn layers(name: &str) {
    let Some(decoded) = intact(vector(name)) else {
        return;
    };
    match &decoded {
        Message::Sign(sign) => sign.signatures.iter().for_each(|signer| {
            round_trip(&encode(signer).expect("encodes"), |bytes| {
                reencode::<Signature>(bytes)
            });
        }),
        Message::Encrypt(Encrypt { recipients, .. }) | Message::Mac(Mac { recipients, .. }) => {
            recipients.iter().for_each(|recipient| {
                round_trip(&encode(recipient).expect("encodes"), |bytes| {
                    reencode::<Recipient<Recipient>>(bytes)
                });
            });
        }
        Message::Sign1(_) | Message::Encrypt0(_) | Message::Mac0(_) => {}
    }
}

fn keys_and_sets(name: &str) {
    let keys = keys(vector(name));
    for (_, raw) in &keys {
        round_trip(&reencode::<Key>(raw).expect("re-encodes"), |bytes| {
            reencode::<Key>(bytes)
        });
    }
    if !keys.is_empty() {
        round_trip(&encode(&key_set(&keys)).expect("encodes"), |bytes| {
            reencode::<KeySet>(bytes)
        });
    }
}

/// The intermediate as its structure's type writes it; the types refuse the AADs whose context RFC
/// 9052 §5.3 does not allow.
fn intermediate_round_trips(name: &str, at: &str, role: Role) {
    let (_, step) = intermediate(name, at, role);
    if role == Role::Aad && NOT_ENC_STRUCTURES.contains(&name) {
        assert!(structure_reencoded(role, step.bytes).is_err());
    } else {
        round_trip(
            &structure_reencoded(role, step.bytes).expect("re-encodes"),
            |bytes| structure_reencoded(role, bytes),
        );
    }
}

macro_rules! cases {
    ($(($test:ident, $name:literal)),* $(,)?) => {
        $(
            mod $test {
                #[test]
                fn message() {
                    super::message($name);
                }

                #[test]
                fn headers() {
                    super::headers($name);
                }

                #[test]
                fn signers_and_recipients() {
                    super::layers($name);
                }

                #[test]
                fn keys() {
                    super::keys_and_sets($name);
                }
            }
        )*
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/cases.rs"));

macro_rules! intermediate_cases {
    ($(($test:ident, $name:literal, $at:literal, $role:ident)),* $(,)?) => {
        mod intermediates {
            use crate::common::vectors::Role;
            $(
                #[test]
                fn $test() {
                    super::intermediate_round_trips($name, $at, Role::$role);
                }
            )*
        }
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/intermediate_cases.rs"));
