//! RFC 9711 Appendix D's CDDL for CWT, with RFC 9052 and RFC 9053's for the COSE rules it names,
//! all extracted from the RFCs' XML, against what the CWT types write: one case per example and
//! rule. The `cddl` crate checks each item against the rule named as the document's root, with
//! the CBOR feature that Appendix D's `JC<J, C>` labels select.

use crate::examples::{Example, Section, claims_payload, example, message, reencoded};

const RFC_9711_APPENDIX_D: &str =
    include_str!(concat!(env!("RFC_CDDL"), "/rfc9711-appendix-d.cddl"));
const RFC_9052: &str = include_str!(concat!(env!("RFC_CDDL"), "/rfc9052.cddl"));
const RFC_9053: &str = include_str!(concat!(env!("RFC_CDDL"), "/rfc9053.cddl"));

/// The CDDL crate's verdict on `item` against `rule`.
fn matches(rule: &str, item: &[u8]) -> Result<(), String> {
    cddl::validate_cbor_from_slice(
        &format!("cbor-static-root = {rule}\n{RFC_9711_APPENDIX_D}\n{RFC_9052}\n{RFC_9053}"),
        item,
        Some(&["cbor"]),
    )
    .map_err(|error| error.to_string())
}

/// The example, as the types re-encode it, matches the rule for what it is.
fn matches_its_rule(section: Section, input: &[u8]) {
    let rule = match example(section) {
        Example::ClaimsSet => "Claims-Set",
        Example::Key => "COSE_Key",
        Example::TaggedCwt => "CWT-Tagged-Message",
        Example::UntaggedCwt => "CWT-Untagged-Message",
    };
    assert_eq!(
        matches(rule, &reencoded(section, input).expect("re-encodes")),
        Ok(())
    );
}

/// `CWT-Messages`, the union of both forms, takes every CWT example and no claims set or key.
fn is_a_cwt_message_exactly_when_it_is_one(section: Section, input: &[u8]) {
    assert_eq!(
        matches(
            "CWT-Messages",
            &reencoded(section, input).expect("re-encodes")
        )
        .is_ok(),
        matches!(example(section), Example::TaggedCwt | Example::UntaggedCwt)
    );
}

/// The signed and authenticated examples' claims sets, as the types re-encode them, match
/// `Claims-Set`; the other examples carry none in the clear.
fn payload_matches(section: Section, input: &[u8]) {
    use cbor_static_conformance::cwt::Claims;

    use crate::common::reencode;

    let payload = message(section, input).as_ref().and_then(claims_payload);
    assert_eq!(
        payload.is_some(),
        matches!(section, Section::A3 | Section::A4 | Section::A7)
    );
    if let Some(payload) = payload {
        assert_eq!(
            matches(
                "Claims-Set",
                &reencode::<Claims>(payload).expect("re-encodes")
            ),
            Ok(())
        );
    }
}

macro_rules! rfc8392_cases {
    ($(($test:ident, $section:literal, $variant:ident)),* $(,)?) => {
        $(
            mod $test {
                use super::Section;

                #[test]
                fn its_rule() {
                    super::matches_its_rule(Section::$variant, rfc8392!($test));
                }

                #[test]
                fn cwt_messages() {
                    super::is_a_cwt_message_exactly_when_it_is_one(Section::$variant, rfc8392!($test));
                }

                #[test]
                fn claims_payload() {
                    super::payload_matches(Section::$variant, rfc8392!($test));
                }
            }
        )*
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc8392_cases.rs"));

/// RFC 8392 §3.1.1 makes `iss` a `StringOrURI`, and the typed claims reject an integer one.
/// Appendix D's `Claims-Set` admits it: its extension point
/// `* Claim-Label .feature "extended-claims-label" => any` takes any label with any value, and
/// RFC 9165 §4 lets a validator only report the feature it marks. Appendix D's introduction:
/// "This, however, is NOT a normative or standard definition of CWT or JWT in CDDL. The prose in
/// CWT and JWT remains the normative definition." So the typed claims follow RFC 8392.
#[test]
fn the_cddl_admits_an_integer_issuer_and_the_typed_claims_do_not() {
    use cbor_static::ErrorKind;
    use cbor_static_conformance::cwt::Claims;

    let integer_issuer = b"\xa1\x01\x01";
    assert_eq!(
        (
            matches("Claims-Set", integer_issuer),
            cbor_static::from_slice::<Claims>(integer_issuer)
                .map(|_| ())
                .map_err(|e| e.kind),
        ),
        (Ok(()), Err(ErrorKind::TypeMismatch))
    );
}

/// RFC 8392 §3.1.3's array audience, as the typed claims write it, matches `Claims-Set` too,
/// though Appendix D types the audience as text: the extension point admits it.
#[test]
fn the_cddl_admits_an_array_audience() {
    use cbor_static_conformance::cwt::{Audience, Claims};

    use crate::common::encode;

    assert_eq!(
        matches(
            "Claims-Set",
            &encode(&Claims {
                iss: None,
                sub: None,
                aud: Some(Audience::Many(
                    heapless::Vec::from_slice(&["a", "b"]).expect("fits")
                )),
                exp: None,
                nbf: None,
                iat: None,
                cti: None,
                other: heapless::Vec::new(),
            })
            .expect("encodes")
        ),
        Ok(())
    );
}

/// A control: a claims set is a map.
#[test]
fn an_array_is_not_a_claims_set() {
    assert!(matches("Claims-Set", b"\x81\x01").is_err());
}
