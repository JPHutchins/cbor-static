//! RFC 9052 and RFC 9053's CDDL, extracted from the RFCs' XML, against every item the COSE types
//! write: one case per example and rule, and one per intermediate. The `cddl` crate checks each
//! item against the rule named as the document's root.

use cbor_static_conformance::cose::Message;

use crate::common::encode;
use crate::common::vectors::{COSE_WG, Failure, Kind, Role, Vector, intermediate, vector};
use crate::decoded::{intact, layers, reencoded};
use crate::headers::maps;
use crate::keys::{key_set, keys};

const RFC_9052: &str = include_str!(concat!(env!("RFC_CDDL"), "/rfc9052.cddl"));
const RFC_9053: &str = include_str!(concat!(env!("RFC_CDDL"), "/rfc9053.cddl"));

/// The CDDL crate's verdict on `item` against `rule` of RFC 9052 and RFC 9053.
fn matches(rule: &str, item: &[u8]) -> Result<(), String> {
    cddl::validate_cbor_from_slice(
        &format!("cbor-static-root = {rule}\n{RFC_9052}\n{RFC_9053}"),
        item,
        None,
    )
    .map_err(|error| error.to_string())
}

fn all_match(rule: &str, items: &[Vec<u8>]) {
    for item in items {
        assert_eq!(matches(rule, item), Ok(()), "{rule}: {item:02x?}");
    }
}

const fn untagged_rule(kind: Kind) -> &'static str {
    match kind {
        Kind::Sign => "COSE_Sign",
        Kind::Sign1 => "COSE_Sign1",
        Kind::Encrypt => "COSE_Encrypt",
        Kind::Encrypt0 => "COSE_Encrypt0",
        Kind::Mac => "COSE_Mac",
        Kind::Mac0 => "COSE_Mac0",
    }
}

/// The message as written matches the rules for its form: tagged, or untagged where the generator
/// removed the tag. A changed tag matches no tagged message.
fn message_matches(name: &str) {
    let vector = vector(name);
    let ops = vector.expect.ops();
    if ops.contains(&Failure::ChangeCborTag) {
        assert!(matches("COSE_Tagged_Message", vector.output).is_err());
    } else if ops.contains(&Failure::RemoveCborTag) {
        for rule in ["COSE_Untagged_Message", untagged_rule(vector.kind)] {
            assert_eq!(
                matches(rule, &reencoded(vector).expect("re-encodes")),
                Ok(()),
                "{rule}"
            );
        }
    } else {
        for rule in [
            "COSE_Messages",
            "COSE_Tagged_Message",
            &format!("{rule}_Tagged", rule = untagged_rule(vector.kind)),
        ] {
            assert_eq!(
                matches(rule, &reencoded(vector).expect("re-encodes")),
                Ok(()),
                "{rule}"
            );
        }
    }
}

/// Every layer's header maps match `header_map`, and its protected byte string
/// `empty_or_serialized_map`.
fn headers_match(name: &str) {
    all_match(
        "header_map",
        &maps(vector(name))
            .into_iter()
            .map(|(_, map)| map)
            .collect::<Vec<_>>(),
    );
    all_match(
        "empty_or_serialized_map",
        &intact(vector(name))
            .map_or_default(|decoded| layers(&decoded))
            .iter()
            .map(|layer| encode(&layer.protected).expect("encodes"))
            .collect::<Vec<_>>(),
    );
}

/// Signers match `COSE_Signature` and recipients `COSE_recipient`.
fn layers_match(name: &str) {
    let Some(decoded) = intact(vector(name)) else {
        return;
    };
    let (signatures, recipients) = match &decoded {
        Message::Sign(sign) => (
            sign.signatures
                .iter()
                .map(|s| encode(s).expect("encodes"))
                .collect(),
            Vec::new(),
        ),
        Message::Encrypt(encrypt) => (
            Vec::new(),
            encrypt
                .recipients
                .iter()
                .map(|r| encode(r).expect("encodes"))
                .collect(),
        ),
        Message::Mac(mac) => (
            Vec::new(),
            mac.recipients
                .iter()
                .map(|r| encode(r).expect("encodes"))
                .collect(),
        ),
        Message::Sign1(_) | Message::Encrypt0(_) | Message::Mac0(_) => (Vec::new(), Vec::new()),
    };
    all_match("COSE_Signature", &signatures);
    all_match("COSE_recipient", &recipients);
}

/// The keys in the headers match `COSE_Key`, and together `COSE_KeySet`.
fn keys_match(name: &str) {
    let keys = keys(vector(name));
    all_match(
        "COSE_Key",
        &keys.iter().map(|(_, key)| key.to_vec()).collect::<Vec<_>>(),
    );
    if !keys.is_empty() {
        all_match("COSE_KeySet", &[encode(&key_set(&keys)).expect("encodes")]);
    }
}

macro_rules! cases {
    ($(($test:ident, $name:literal)),* $(,)?) => {
        $(
            mod $test {
                #[test]
                fn message() {
                    super::message_matches($name);
                }

                #[test]
                fn headers() {
                    super::headers_match($name);
                }

                #[test]
                fn signers_and_recipients() {
                    super::layers_match($name);
                }

                #[test]
                fn keys() {
                    super::keys_match($name);
                }
            }
        )*
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/cases.rs"));

/// Why RFC 9052's CDDL rejects an intermediate the types build byte for byte, if it does.
#[derive(Debug, PartialEq, Eq)]
enum Rejected {
    /// An AAD whose context RFC 9052 §5.3 does not allow.
    NotAnEncStructure,
    /// A countersignature structure: RFC 8152's and RFC 9338's, not RFC 9052's.
    Countersignature,
    /// `Sig_structure`'s optional `sign_protected` before two required byte strings: a
    /// four-element `Signature1` structure is read as having one when its external AAD could be a
    /// protected header, and then lacks its payload. Both the `cddl` crate and the Ruby reference
    /// tool reject it (issue #78).
    Signature1ExternalAadTakenForProtected,
}

fn rejected(vector: &Vector<Kind>, at: &str, role: Role, bytes: &[u8]) -> Option<Rejected> {
    use cbor_static_conformance::cose::HeaderMap;

    use crate::intermediates::NOT_ENC_STRUCTURES;

    match role {
        Role::Aad if NOT_ENC_STRUCTURES.contains(&vector.name) => Some(Rejected::NotAnEncStructure),
        Role::ToBeSign if at.contains("countersign") => Some(Rejected::Countersignature),
        Role::ToBeSign
            if bytes.first() == Some(&0x84)
                && (vector.external_aad.is_empty()
                    || cbor_static::from_slice::<HeaderMap>(vector.external_aad).is_ok()) =>
        {
            Some(Rejected::Signature1ExternalAadTakenForProtected)
        }
        Role::ToBeSign | Role::ToMac | Role::Aad | Role::Context => None,
    }
}

const fn rule(role: Role) -> &'static str {
    match role {
        Role::ToBeSign => "Sig_structure",
        Role::ToMac => "MAC_structure",
        Role::Aad => "Enc_structure",
        Role::Context => "COSE_KDF_Context",
    }
}

/// The intermediate matches its structure's rule, or is rejected for its pinned reason.
fn intermediate_matches(name: &str, at: &str, role: Role) {
    let (vector, step) = intermediate(name, at, role);
    let verdict = matches(rule(role), step.bytes);
    match rejected(vector, at, role, step.bytes) {
        None => assert_eq!(verdict, Ok(())),
        Some(reason) => assert!(verdict.is_err(), "{reason:?} matched"),
    }
}

macro_rules! intermediate_cases {
    ($(($test:ident, $name:literal, $at:literal, $role:ident)),* $(,)?) => {
        mod intermediates {
            use crate::common::vectors::Role;
            $(
                #[test]
                fn $test() {
                    super::intermediate_matches($name, $at, Role::$role);
                }
            )*
        }
    };
}

include!(concat!(env!("COSE_WG_VECTORS"), "/intermediate_cases.rs"));

#[test]
fn the_rejected_intermediates_are_the_pinned_ones() {
    let reasons = COSE_WG
        .iter()
        .flat_map(|vector| {
            vector
                .intermediates
                .iter()
                .filter_map(move |step| rejected(vector, step.at, step.role, step.bytes))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        [
            Rejected::NotAnEncStructure,
            Rejected::Countersignature,
            Rejected::Signature1ExternalAadTakenForProtected,
        ]
        .map(|reason| reasons.iter().filter(|r| **r == reason).count()),
        [9, 30, 20]
    );
}

#[test]
fn rfc_9528_as_published_does_not_parse_and_its_erratum_does() {
    let edhoc = |file: &str| {
        cddl::cddl_from_str(
            &format!(
                "{}\n{RFC_9052}",
                std::fs::read_to_string(format!("{dir}/{file}", dir = env!("RFC_CDDL")))
                    .expect("reads")
            ),
            false,
        )
        .map(|_| ())
    };
    assert!(edhoc("rfc9528.cddl").is_err());
    assert_eq!(edhoc("rfc9528-erratum-8258.cddl"), Ok(()));
}

#[test]
fn rfc_9711_appendix_d_parses() {
    let eat = include_str!(concat!(env!("RFC_CDDL"), "/rfc9711-appendix-d.cddl"));
    assert_eq!(
        cddl::cddl_from_str(&format!("{eat}\n{RFC_9052}"), false).map(|_| ()),
        Ok(())
    );
}

/// RFC 9052's CDDL, which §1.4 calls informational, admits a negative content type
/// (`tstr / int`); §3.1's normative table gives `tstr / uint`, and the typed headers follow the
/// table.
#[test]
fn the_cddl_admits_a_negative_content_type_and_the_typed_headers_do_not() {
    use cbor_static::ErrorKind;
    use cbor_static_conformance::cose::Headers;

    let negative = b"\xa1\x03\x20";
    assert_eq!(
        (
            matches("header_map", negative),
            cbor_static::from_slice::<Headers>(negative)
                .map(|_| ())
                .map_err(|e| e.kind),
        ),
        (Ok(()), Err(ErrorKind::TypeMismatch))
    );
}
