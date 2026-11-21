//! The structures signatures, MACs, AEADs and key derivations are computed over, built from the
//! decoded message's parts and compared with the examples' intermediates byte for byte.
//!
//! Key sizes for `COSE_KDF_Context`'s keyDataLength are RFC 9053's (RFC Editor, Simplified BSD):
//! Table 3 (HMAC, §3.1, the hash's width), Table 4 (AES-MAC, §3.2), Table 5 (AES-GCM, §4.1),
//! Table 6 (AES-CCM, §4.2), Table 7 (ChaCha20/Poly1305, §4.3) and Table 13 (AES Key Wrap,
//! §6.2.1). Table 16 (§6.4.1) gives the key wrap each ECDH-with-key-wrap algorithm derives for.

use cbor_static::{ErrorKind, RawCbor};
use cbor_static_conformance::cose::{
    BytesOrNil, EncContext, EncStructure, HeaderMap, KdfContext, Label, MacContext, MacStructure,
    Message, Nonce, PartyInfo, Protected, SigContext, SigStructure, SigStructure1, Signature,
};

use crate::common::vectors::{
    COSE_WG, Failure, Intermediate, Kind, Role, Unsent, Vector, intermediate,
};
use crate::common::{encode, reencode};
use crate::decoded::{Layer, layers, message};

/// Additional authenticated data whose context string RFC 9052 §5.3 does not allow.
const NOT_ENC_STRUCTURES: [&str; 9] = [
    "chacha-poly-examples/chacha-poly-enc-01",
    "hkdf-aes-examples/hmac-aes-128-03",
    "hkdf-aes-examples/hmac-aes-128-04",
    "hkdf-aes-examples/hmac-aes-256-03",
    "hkdf-aes-examples/hmac-aes-256-04",
    "hkdf-hmac-sha-examples/hmac-sha-256-03",
    "hkdf-hmac-sha-examples/hmac-sha-256-04",
    "hkdf-hmac-sha-examples/hmac-sha-512-03",
    "hkdf-hmac-sha-examples/hmac-sha-512-04",
];

/// Intermediates the generator computed for a message other than the one it wrote: the
/// unaltered payload of a Sign1 whose payload it then altered, and a countersignature structure
/// for header 7 on a message that carries header 9.
const NOT_OF_THIS_MESSAGE: [(&str, &str); 2] = [
    ("sign1-tests/sign-fail-02", ""),
    ("countersign1/mac0-01", "countersign0.0"),
];

/// The generator's operations that rewrite a protected header after the intermediates are
/// computed.
const REWRITES_PROTECTED: [Failure; 4] = [
    Failure::ChangeAttr,
    Failure::AddProtected,
    Failure::RemoveProtected,
    Failure::ChangeProtected,
];

fn of_another_message(vector: &Vector<Kind>, step: &Intermediate) -> bool {
    vector
        .expect
        .ops()
        .iter()
        .any(|op| REWRITES_PROTECTED.contains(op))
        || NOT_OF_THIS_MESSAGE.contains(&(vector.name, step.at))
}

const KEY_BITS: [(i64, u64); 23] = [
    (4, 256),
    (5, 256),
    (6, 384),
    (7, 512),
    (14, 128),
    (15, 256),
    (25, 128),
    (26, 256),
    (1, 128),
    (2, 192),
    (3, 256),
    (10, 128),
    (11, 256),
    (12, 128),
    (13, 256),
    (30, 128),
    (31, 256),
    (32, 128),
    (33, 256),
    (24, 256),
    (-3, 128),
    (-4, 192),
    (-5, 256),
];

const KEY_WRAPPED_BY: [(i64, i64); 6] = [
    (-29, -3),
    (-30, -4),
    (-31, -5),
    (-32, -3),
    (-33, -4),
    (-34, -5),
];

const PARTY_U: [i64; 3] = [-21, -22, -23];
const PARTY_V: [i64; 3] = [-24, -25, -26];

/// What one intermediate must be.
#[derive(Debug)]
enum Built {
    Bytes(Vec<u8>),
    Context(Vec<String>),
}

fn every_intermediate() -> impl Iterator<Item = (&'static Vector<Kind>, &'static Intermediate)> {
    COSE_WG
        .iter()
        .flat_map(|vector| vector.intermediates.iter().map(move |step| (vector, step)))
}

fn not_an_enc_structure(vector: &Vector<Kind>, step: &Intermediate) -> bool {
    step.role == Role::Aad && NOT_ENC_STRUCTURES.contains(&vector.name)
}

/// The intermediate decodes as its structure and re-encodes byte for byte, and equals the
/// structure built from the decoded message, unless it is one of the pinned exceptions: an AAD
/// RFC 9052 does not allow is rejected, and one computed for another message differs.
fn rebuilt_from_its_message(name: &str, at: &str, role: Role) {
    let (vector, step) = intermediate(name, at, role);
    if not_an_enc_structure(vector, step) {
        assert_eq!(
            cbor_static::from_slice::<EncStructure>(step.bytes)
                .map(|_| ())
                .map_err(|e| e.kind),
            Err(ErrorKind::UnknownVariant)
        );
        return;
    }
    let again = match (role, step.bytes.first()) {
        (Role::ToBeSign, Some(0x85)) => reencode::<SigStructure>(step.bytes),
        (Role::ToBeSign, _) => reencode::<SigStructure1>(step.bytes),
        (Role::ToMac, _) => reencode::<MacStructure>(step.bytes),
        (Role::Aad, _) => reencode::<EncStructure>(step.bytes),
        (Role::Context, _) => reencode::<KdfContext>(step.bytes),
    };
    assert_eq!(again.as_deref(), Ok(step.bytes));
    if vector.expect.ops().contains(&Failure::ChangeCborTag) {
        assert!(
            message(vector).is_err(),
            "a broken tag leaves no message to build from"
        );
        return;
    }
    let decoded = message(vector).expect("decodes");
    match (
        built(vector, &decoded, &layers(&decoded), step),
        of_another_message(vector, step),
    ) {
        (Ok(Built::Bytes(bytes)), altered) => {
            assert_eq!(bytes == step.bytes, !altered, "{bytes:02x?}");
        }
        (Ok(Built::Context(mismatches)), false) => assert_eq!(mismatches, Vec::<String>::new()),
        (other, altered) => panic!("altered: {altered}, built: {other:?}"),
    }
}

macro_rules! intermediate_cases {
    ($(($test:ident, $name:literal, $at:literal, $role:ident)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                rebuilt_from_its_message($name, $at, Role::$role);
            }
        )*
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/intermediate_cases.rs"));

/// The intermediates of altered messages: six message types, each with two `ChangeAttr`, one
/// `AddProtected`, one `RemoveProtected` and one `ChangeProtected` example, and the two named.
const ALTERED: usize = 6 * 5 + NOT_OF_THIS_MESSAGE.len();

#[test]
fn every_intermediate_is_built_from_its_message_but_the_pinned_exceptions() {
    assert_eq!(
        (
            every_intermediate().count(),
            every_intermediate()
                .filter(|(vector, step)| not_an_enc_structure(vector, step))
                .count(),
            every_intermediate()
                .filter(|(vector, step)| !not_an_enc_structure(vector, step))
                .filter(|(vector, step)| of_another_message(vector, step))
                .count(),
        ),
        (470, NOT_ENC_STRUCTURES.len(), ALTERED)
    );
    assert_eq!(
        every_intermediate()
            .filter(|(vector, _)| vector.expect.ops().contains(&Failure::ChangeCborTag))
            .count(),
        6
    );
}

/// The intermediate rebuilt from the message.
fn built(
    vector: &Vector<Kind>,
    decoded: &Message<'static>,
    walked: &[Layer],
    step: &Intermediate,
) -> Result<Built, String> {
    let message_layer = &walked[0];
    match (&step.role, step.at, decoded) {
        (Role::ToBeSign, "", Message::Sign1(sign1)) => bytes(&SigStructure1 {
            context: SigContext::Signature1,
            body_protected: sign1.protected,
            external_aad: vector.external_aad,
            payload: message_layer.content,
        }),
        (Role::ToMac, "", Message::Mac(_) | Message::Mac0(_)) => bytes(&MacStructure {
            context: if matches!(decoded, Message::Mac(_)) {
                MacContext::Mac
            } else {
                MacContext::Mac0
            },
            protected: message_layer.protected,
            external_aad: vector.external_aad,
            payload: message_layer.content,
        }),
        (Role::Aad, "", Message::Encrypt(_) | Message::Encrypt0(_)) => bytes(&EncStructure {
            context: if matches!(decoded, Message::Encrypt(_)) {
                EncContext::Encrypt
            } else {
                EncContext::Encrypt0
            },
            protected: message_layer.protected,
            external_aad: vector.external_aad,
        }),
        (Role::ToBeSign, at, _) => match at.rsplit_once('.').and_then(|(rest, index)| {
            rest.rsplit_once('.')
                .map_or(Some(("", rest, index)), |(layer, kind)| {
                    Some((layer, kind, index))
                })
        }) {
            Some((layer, "countersigners", index)) => {
                countersigned(vector, walked, layer, Countersignature::Full, index)
            }
            Some((layer, "countersign0", index)) => {
                countersigned(vector, walked, layer, Countersignature::Bare, index)
            }
            _ => signed(vector, message_layer, walked, at),
        },
        (Role::Context, at, _) => context(vector, walked, at).map(Built::Context),
        _ => Err(String::from("no rule builds this intermediate")),
    }
}

fn bytes<T: cbor_static::Encode>(value: &T) -> Result<Built, String> {
    encode(value)
        .map(Built::Bytes)
        .map_err(|error| format!("{error:?}"))
}

fn layer<'w>(walked: &'w [Layer], at: &str) -> Result<&'w Layer, String> {
    walked
        .iter()
        .find(|layer| layer.at == at)
        .ok_or_else(|| format!("no layer at {at:?}"))
}

fn signed(
    vector: &Vector<Kind>,
    message_layer: &Layer,
    walked: &[Layer],
    at: &str,
) -> Result<Built, String> {
    bytes(&SigStructure {
        context: SigContext::Signature,
        body_protected: message_layer.protected,
        sign_protected: layer(walked, at)?.protected,
        external_aad: vector.external_aad,
        payload: message_layer.content,
    })
}

/// Header 7 holds `COSE_Signature`s (RFC 8152 §4.5), header 9 bare signatures with no protected
/// header of their own (RFC 9338's version 1).
#[derive(Clone, Copy)]
enum Countersignature {
    Full,
    Bare,
}

/// A countersignature of the layer at `at`.
fn countersigned(
    vector: &Vector<Kind>,
    walked: &[Layer],
    at: &str,
    kind: Countersignature,
    index: &str,
) -> Result<Built, String> {
    let signed = layer(walked, at)?;
    let index = index.parse::<usize>().map_err(|error| error.to_string())?;
    let (context, sign_protected) = match kind {
        Countersignature::Full => (
            SigContext::CounterSignature,
            countersignatures(header(signed, 7)?)?
                .get(index)
                .map(|signature| signature.protected)
                .ok_or("too few countersignatures")?,
        ),
        Countersignature::Bare => {
            header(signed, 9)?;
            (SigContext::CounterSignature0, Protected(b""))
        }
    };
    bytes(&SigStructure {
        context,
        body_protected: signed.protected,
        sign_protected,
        external_aad: vector.external_aad,
        payload: signed.content,
    })
}

fn header(layer: &Layer, label: i64) -> Result<RawCbor<'static>, String> {
    layer
        .unprotected
        .get(&Label::Int(label))
        .copied()
        .ok_or_else(|| format!("{at}: no header {label}", at = layer.at))
}

fn countersignatures(value: RawCbor<'static>) -> Result<Vec<Signature<'static>>, String> {
    cbor_static::from_slice::<Signature>(value.as_bytes())
        .map(|one| vec![one])
        .or_else(|_| {
            cbor_static::from_slice::<heapless::Vec<Signature, 4>>(value.as_bytes())
                .map(|many| many.into_iter().collect())
        })
        .map_err(|error: cbor_static::Error| format!("{error:?}"))
}

/// Each field of the recipient's KDF context against its source: the algorithm the derived key
/// is for, that algorithm's key size, the recipient's protected header, and `PartyInfo` from the
/// recipient's headers or, for parts not sent, the example's input.
fn context(vector: &Vector<Kind>, walked: &[Layer], at: &str) -> Result<Vec<String>, String> {
    let step = vector
        .intermediates
        .iter()
        .find(|step| step.at == at && matches!(step.role, Role::Context))
        .ok_or("no context")?;
    let context: KdfContext =
        cbor_static::from_slice(step.bytes).map_err(|error| format!("{error:?}"))?;
    let recipient = layer(walked, at)?;
    let parent = layer(
        walked,
        at.rsplit_once(".recipients.")
            .map_or("", |(parent, _)| parent),
    )?;
    let unsent = vector.unsent.iter().find(|unsent| unsent.at == at);
    let derived_for = KEY_WRAPPED_BY
        .iter()
        .find(|&&(ecdh, _)| Ok(ecdh) == algorithm(recipient))
        .map_or_else(|| algorithm(parent), |&(_, wrap)| Ok(wrap))?;
    let checks = [
        (
            "AlgorithmID",
            context.algorithm_id == Label::Int(derived_for),
        ),
        (
            "keyDataLength",
            KEY_BITS
                .iter()
                .find(|&&(alg, _)| alg == derived_for)
                .map(|&(_, bits)| bits)
                == Some(context.supp_pub_info.key_data_length),
        ),
        (
            "SuppPubInfo.protected",
            context.supp_pub_info.protected == recipient.protected,
        ),
        (
            "SuppPubInfo.other",
            context.supp_pub_info.other == unsent.and_then(|unsent| unsent.pub_other),
        ),
        (
            "SuppPrivInfo",
            context.supp_priv_info == unsent.and_then(|unsent| unsent.priv_other),
        ),
        (
            "PartyUInfo",
            party(&context.party_u_info)?
                == expected_party(recipient, PARTY_U, unsent.and_then(|u| u.apu_id))?,
        ),
        (
            "PartyVInfo",
            party(&context.party_v_info)?
                == expected_party(recipient, PARTY_V, unsent.and_then(|u| u.apv_id))?,
        ),
    ];
    Ok(checks
        .into_iter()
        .filter(|&(_, ok)| !ok)
        .map(|(field, _)| String::from(field))
        .collect())
}

/// The header a recipient sent under `label`, as written.
fn sent(recipient: &Layer, label: i64) -> Option<Vec<u8>> {
    recipient
        .unprotected
        .get(&Label::Int(label))
        .map(|raw| raw.as_bytes().to_vec())
        .or_else(|| {
            recipient
                .protected
                .headers()
                .ok()?
                .get(&Label::Int(label))
                .map(|raw| raw.as_bytes().to_vec())
        })
}

/// A party's identity, nonce and other as the recipient sent them, the identity from the input
/// when it is not sent, and nil otherwise.
fn expected_party(
    recipient: &Layer,
    labels: [i64; 3],
    unsent_id: Option<&'static [u8]>,
) -> Result<Vec<Vec<u8>>, String> {
    labels
        .iter()
        .enumerate()
        .map(|(position, &label)| {
            sent(recipient, label)
                .map_or_else(
                    || match (position, unsent_id) {
                        (0, Some(id)) => encode(&id),
                        _ => encode(&()),
                    },
                    Ok,
                )
                .map_err(|error| format!("{error:?}"))
        })
        .collect()
}

fn party(info: &PartyInfo) -> Result<Vec<Vec<u8>>, String> {
    [
        encode(&info.identity),
        encode(&info.nonce),
        encode(&info.other),
    ]
    .into_iter()
    .collect::<Result<_, _>>()
    .map_err(|error| format!("{error:?}"))
}

fn algorithm(layer: &Layer) -> Result<i64, String> {
    let protected: HeaderMap = layer
        .protected
        .headers()
        .map_err(|error| format!("{error:?}"))?;
    protected
        .get(&Label::Int(1))
        .or_else(|| layer.unprotected.get(&Label::Int(1)))
        .ok_or_else(|| format!("{at}: no alg", at = layer.at))
        .and_then(|raw| {
            cbor_static::from_slice::<i64>(raw.as_bytes()).map_err(|error| format!("{error:?}"))
        })
}

#[test]
fn the_unsent_parts_name_recipients() {
    assert!(COSE_WG.iter().all(|vector| {
        vector
            .unsent
            .iter()
            .all(|Unsent { at, .. }| at.starts_with("recipients."))
    }));
}

#[test]
fn a_nonce_is_bytes_an_integer_or_nil() {
    assert_eq!(
        [b"\x41\x01".as_slice(), b"\x01", b"\xf6"].map(cbor_static::from_slice::<Nonce>),
        [
            Ok(Nonce::Bytes(b"\x01")),
            Ok(Nonce::Int(1)),
            Ok(Nonce::Nil(()))
        ]
    );
    assert_eq!(
        cbor_static::from_slice::<BytesOrNil>(b"\x01").map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
}
