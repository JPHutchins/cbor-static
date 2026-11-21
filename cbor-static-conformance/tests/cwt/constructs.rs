//! Every CDDL construct a CWT uses, one test each: RFC 9711 Appendix D's CDDL for RFC 8392's claims
//! and messages, which RFC 8392 itself predates. How the CWT types express each one, or, where the
//! derive cannot yet, the nearest form.

use cbor_static::ErrorKind;
use cbor_static_conformance::cose::Message;
use cbor_static_conformance::cwt::{Audience, Claims, NumericDate, Tagged};

use crate::common::{encode, reencode};

/// `Claims-Set = { * $$Claims-Set-Claims, * Claim-Label => any }`: the claims the socket names
/// are fields, and any other claim is captured as written, in place.
#[test]
fn a_claims_set_is_its_registered_claims_and_the_rest_as_written() {
    let with_confirmation = b"\xa2\x02\x61\x61\x08\xa1\x01\xa0";
    assert_eq!(
        cbor_static::from_slice::<Claims>(with_confirmation)
            .map(|claims| (claims.sub, claims.other.len())),
        Ok((Some("a"), 1))
    );
    assert_eq!(
        reencode::<Claims>(with_confirmation).as_deref(),
        Ok(with_confirmation.as_slice())
    );
}

/// `iss-claim-label = JC<"iss", 1>`, `cti-claim-label = CBOR-ONLY<7>`: the generics and
/// `.feature` choose between JSON and CBOR; in CBOR each claim is its integer key.
#[test]
fn a_claim_label_is_its_integer_key() {
    let claims = Claims {
        iss: Some("a"),
        sub: None,
        aud: None,
        exp: None,
        nbf: None,
        iat: None,
        cti: Some(b"\x0b"),
        other: heapless::Vec::new(),
    };
    assert_eq!(
        encode(&claims).as_deref(),
        Ok(b"\xa2\x01\x61a\x07\x41\x0b".as_slice())
    );
}

/// `aud-claim-label => string-or-uri`, with `string-or-uri = text`: Appendix D types the audience
/// as text, while RFC 8392 §3.1.3 also allows an array of them. The typed claims follow RFC 8392:
/// an untagged choice of text or an array.
#[test]
fn an_audience_is_text_or_an_array_of_text() {
    assert_eq!(
        [
            b"\xa1\x03\x61a".as_slice(),
            b"\xa1\x03\x81\x61a",
            b"\xa1\x03\x01"
        ]
        .map(|input| {
            cbor_static::from_slice::<Claims>(input)
                .map(|claims| claims.aud)
                .map_err(|e| e.kind)
        }),
        [
            Ok(Some(Audience::One("a"))),
            Ok(Some(Audience::Many(
                heapless::Vec::from_slice(&["a"]).expect("fits")
            ))),
            Err(ErrorKind::TypeMismatch)
        ]
    );
}

/// `exp-claim-label => ~time`: the number tag 1 would wrap, without the tag; an integer or a float.
#[test]
fn a_time_without_its_tag_is_an_int_or_a_float() {
    assert_eq!(
        [b"\x01".as_slice(), b"\xf9\x3c\x00", b"\xc1\x01"]
            .map(|input| cbor_static::from_slice::<NumericDate>(input).map_err(|e| e.kind)),
        [
            Ok(NumericDate::Int(1)),
            Ok(NumericDate::Float(1.0)),
            Err(ErrorKind::TypeMismatch)
        ]
    );
}

/// `~time`'s integers span CBOR's `int`, beyond `i64`.
#[test]
fn a_time_outside_i64_is_out_of_range() {
    assert_eq!(
        cbor_static::from_slice::<NumericDate>(b"\x3b\xff\xff\xff\xff\xff\xff\xff\xff")
            .map_err(|e| e.kind),
        Err(ErrorKind::OutOfRange)
    );
}

/// `CWT-Tagged-Message = #6.61(COSE_Tagged_Message)`: a tag over a tag-dispatched choice.
#[test]
fn the_cwt_tag_is_a_tag_over_the_tagged_message() {
    let tagged = b"\xd8\x3d\xd2\x84\x40\xa0\xf6\x40";
    assert!(matches!(
        cbor_static::from_slice::<Tagged>(tagged),
        Ok(cbor_static::Tagged(Message::Sign1(_)))
    ));
    assert_eq!(reencode::<Tagged>(tagged).as_deref(), Ok(tagged.as_slice()));
}

/// `CWT-Messages = CWT-Tagged-Message / CWT-Untagged-Message`: untagged dispatch takes the
/// tag-dispatched `Message` to start with any tag, so it collides with the CWT tag (the
/// compile-fail doctest on `cwt::Tagged`). The nearest form is two entry points that the context
/// chooses between (#64).
#[test]
fn an_optional_cwt_tag_is_two_entry_points() {
    let tagged = b"\xd8\x3d\xd2\x84\x40\xa0\xf6\x40";
    assert_eq!(
        (
            cbor_static::from_slice::<Message>(tagged).map_err(|e| e.kind),
            cbor_static::from_slice::<Tagged>(&tagged[2..]).map_err(|e| e.kind),
        ),
        (Err(ErrorKind::UnexpectedTag), Err(ErrorKind::UnexpectedTag))
    );
}

/// Fixed tags are told apart by their number, even behind one head byte (`d8` for both 61 and
/// 98): what collides with the CWT tag is a tag-dispatched enum, not a shared head byte.
#[test]
fn fixed_tags_that_share_a_head_byte_are_told_apart() {
    use cbor_static::{Cbor, Tagged};

    #[derive(Cbor, Debug, PartialEq)]
    #[serde(untagged)]
    enum Either {
        Cwt(Tagged<61, u8>),
        Sign(Tagged<98, u8>),
    }

    assert_eq!(
        [b"\xd8\x3d\x01".as_slice(), b"\xd8\x62\x01"].map(cbor_static::from_slice::<Either>),
        [Ok(Either::Cwt(Tagged(1))), Ok(Either::Sign(Tagged(1)))]
    );
}

/// RFC 8392 §7.1: the payload is the claims set's encoding, `bstr .cbor Claims-Set` without tag
/// 24. The nearest form is the payload's bytes and a second decode (#64).
#[test]
fn a_claims_set_in_a_payload_is_decoded_a_second_time() {
    use cbor_static_conformance::cose::{BytesOrNil, Sign1};

    let Ok(Sign1 {
        payload: BytesOrNil::Bytes(payload),
        ..
    }) = cbor_static::from_slice(b"\x84\x40\xa0\x43\xa1\x02\x60\x40")
    else {
        panic!("a Sign1 with a payload")
    };
    assert_eq!(
        cbor_static::from_slice::<Claims>(payload).map(|claims| claims.sub),
        Ok(Some(""))
    );
}

/// RFC 8392 §4's registered claims: `iss` 1, `sub` 2, `aud` 3, `exp` 4, `nbf` 5, `iat` 6 and
/// `cti` 7, each a field written under its key, in that order.
#[test]
fn the_registered_claims_are_keys_one_to_seven() {
    use cbor_static::RawCbor;
    use cbor_static_conformance::cose::Label;
    use heapless::LinearMap;

    let claims = Claims {
        iss: Some("i"),
        sub: Some("s"),
        aud: Some(Audience::One("a")),
        exp: Some(NumericDate::Int(4)),
        nbf: Some(NumericDate::Int(5)),
        iat: Some(NumericDate::Float(6.5)),
        cti: Some(b"\x07"),
        other: heapless::Vec::new(),
    };
    let written = encode(&claims).expect("encodes");
    assert_eq!(
        cbor_static::from_slice::<LinearMap<Label, RawCbor, 8>>(&written)
            .map(|map| map.keys().copied().collect::<Vec<_>>()),
        Ok((1..=7).map(Label::Int).collect())
    );
    assert_eq!(cbor_static::from_slice::<Claims>(&written), Ok(claims));
}
