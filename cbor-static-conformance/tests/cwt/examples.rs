//! Every example, one case each: it decodes as the type its section names and re-encodes byte for
//! byte.

use cbor_static::Error;
use cbor_static_conformance::cose::Message;

use crate::common::reencode;

/// An RFC 8392 Appendix A section with a hex example, as `nix/rfc-vectors.nix` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    A1,
    A21,
    A22,
    A23,
    A3,
    A4,
    A5,
    A6,
    A7,
}

/// What a section's example is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Example {
    ClaimsSet,
    Key,
    TaggedCwt,
    UntaggedCwt,
}

pub const fn example(section: Section) -> Example {
    match section {
        Section::A1 => Example::ClaimsSet,
        Section::A21 | Section::A22 | Section::A23 => Example::Key,
        Section::A4 => Example::TaggedCwt,
        Section::A3 | Section::A5 | Section::A6 | Section::A7 => Example::UntaggedCwt,
    }
}

/// The example as its section's type re-encodes it: the claims set, a key's map as written, a
/// message under the CWT tag, or an untagged CWT's message.
pub fn reencoded(section: Section, input: &[u8]) -> Result<Vec<u8>, Error> {
    use cbor_static::RawCbor;
    use cbor_static_conformance::cose::Label;
    use cbor_static_conformance::cwt::{Claims, Tagged};
    use heapless::LinearMap;

    match example(section) {
        Example::ClaimsSet => reencode::<Claims>(input),
        Example::Key => reencode::<LinearMap<Label, RawCbor, 8>>(input),
        Example::TaggedCwt => reencode::<Tagged>(input),
        Example::UntaggedCwt => reencode::<Message>(input),
    }
}

/// The COSE message an example's CWT is, under its tag or not.
pub fn message(section: Section, input: &[u8]) -> Option<Message<'_>> {
    match example(section) {
        Example::TaggedCwt => {
            cbor_static::from_slice::<cbor_static_conformance::cwt::Tagged>(input)
                .ok()
                .map(|tagged| tagged.0)
        }
        Example::UntaggedCwt => cbor_static::from_slice(input).ok(),
        Example::ClaimsSet | Example::Key => None,
    }
}

/// The claims set a message carries in the clear: a signed or authenticated payload.
pub fn claims_payload<'a>(message: &Message<'a>) -> Option<&'a [u8]> {
    use cbor_static_conformance::cose::BytesOrNil;

    match message {
        Message::Sign1(sign1) => Some(sign1.payload),
        Message::Mac0(mac0) => Some(mac0.payload),
        Message::Sign(_) | Message::Encrypt(_) | Message::Encrypt0(_) | Message::Mac(_) => None,
    }
    .and_then(|payload| match payload {
        BytesOrNil::Bytes(bytes) => Some(bytes),
        BytesOrNil::Nil(()) => None,
    })
}

macro_rules! rfc8392_cases {
    ($(($test:ident, $section:literal, $variant:ident)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                assert_eq!(
                    reencoded(Section::$variant, rfc8392!($test)).as_deref(),
                    Ok(rfc8392!($test))
                );
            }
        )*
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc8392_cases.rs"));

/// The example's payload is A.1's claims set, byte for byte.
fn carries_the_example_claims_set(section: Section, input: &[u8]) {
    assert_eq!(
        message(section, input).as_ref().and_then(claims_payload),
        Some(rfc8392!(appendix_a_1))
    );
}

#[test]
fn the_signed_example_carries_the_example_claims_set() {
    carries_the_example_claims_set(Section::A3, rfc8392!(appendix_a_3));
}

#[test]
fn the_maced_example_carries_the_example_claims_set() {
    carries_the_example_claims_set(Section::A4, rfc8392!(appendix_a_4));
}

#[test]
fn the_example_claims_set_has_every_registered_claim_and_no_other() {
    use cbor_static_conformance::cwt::Claims;

    let claims: Claims = cbor_static::from_slice(rfc8392!(appendix_a_1)).expect("decodes");
    assert_eq!(
        (
            [claims.iss, claims.sub].map(|claim| claim.is_some()),
            claims.aud.is_some(),
            [claims.exp, claims.nbf, claims.iat].map(|claim| claim.is_some()),
            claims.cti.is_some(),
            claims.other.len(),
        ),
        ([true; 2], true, [true; 3], true, 0)
    );
}

#[test]
fn the_floating_point_example_dates_its_issue_with_a_float() {
    use cbor_static_conformance::cwt::{Claims, NumericDate};

    let payload = message(Section::A7, rfc8392!(appendix_a_7))
        .as_ref()
        .and_then(claims_payload)
        .expect("a payload");
    assert!(matches!(
        cbor_static::from_slice::<Claims>(payload).map(|claims| claims.iat),
        Ok(Some(NumericDate::Float(_)))
    ));
    assert_eq!(reencode::<Claims>(payload).as_deref(), Ok(payload));
}

/// The example is a `COSE_Encrypt0`.
fn is_encrypt0(section: Section, input: &[u8]) {
    assert!(matches!(
        message(section, input),
        Some(Message::Encrypt0(_))
    ));
}

#[test]
fn the_encrypted_example_is_encrypt0() {
    is_encrypt0(Section::A5, rfc8392!(appendix_a_5));
}

#[test]
fn the_nested_example_is_encrypt0() {
    is_encrypt0(Section::A6, rfc8392!(appendix_a_6));
}
