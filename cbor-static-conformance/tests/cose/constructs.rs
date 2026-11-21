//! Every CDDL construct RFC 9052 and RFC 9053 use, one test each: how the COSE types express it,
//! or, where the derive cannot yet, the nearest form and the check it needs after decoding.

use cbor_static::{Cbor, ErrorKind, RawCbor, RawEntry};
use cbor_static_conformance::cose::{
    BytesOrNil, ContentType, EncContext, HeaderMap, Headers, Key, KeySet, Label, Message, Nonce,
    Protected, Recipient, SigStructure, SigStructure1, Sign, Sign1, Signature, SuppPubInfo,
};

use crate::common::{encode, reencode};

/// `COSE_Sign1_Tagged = #6.18(COSE_Sign1)` and the other five: a variant per tag.
#[test]
fn a_tag_selects_the_message() {
    let tagged = b"\xd2\x84\x40\xa0\xf6\x41\x00";
    assert!(matches!(
        cbor_static::from_slice::<Message>(tagged),
        Ok(Message::Sign1(_))
    ));
    assert_eq!(
        reencode::<Message>(tagged).as_deref(),
        Ok(tagged.as_slice())
    );
    assert_eq!(
        cbor_static::from_slice::<Message>(b"\xd1\x84\x40\xa0\xf6\x41\x00")
            .map(|m| matches!(m, Message::Mac0(_))),
        Ok(true)
    );
}

/// `COSE_Untagged_Message = COSE_Sign / COSE_Sign1 / ...`: every alternative is an array, so the
/// first byte cannot choose; the application knows the type from context and decodes that
/// struct.
#[test]
fn an_untagged_message_is_decoded_as_the_type_its_context_names() {
    let untagged = b"\x84\x40\xa0\xf6\x41\x00";
    assert!(cbor_static::from_slice::<Message>(untagged).is_err());
    assert!(cbor_static::from_slice::<Sign1>(untagged).is_ok());
}

/// `label = int / tstr`, `bstr / nil`, `bstr / int / nil`, `uint / tstr`: alternatives that
/// start with different major types are an untagged enum.
#[test]
fn a_type_choice_by_major_type_is_an_untagged_enum() {
    assert_eq!(
        [b"\x20".as_slice(), b"\x61a"].map(cbor_static::from_slice::<Label>),
        [Ok(Label::Int(-1)), Ok(Label::Text("a"))]
    );
    assert_eq!(
        [b"\xf6".as_slice(), b"\x40"].map(cbor_static::from_slice::<BytesOrNil>),
        [Ok(BytesOrNil::Nil(())), Ok(BytesOrNil::Bytes(b""))]
    );
    assert_eq!(cbor_static::from_slice::<Nonce>(b"\x01"), Ok(Nonce::Int(1)));
    assert_eq!(
        cbor_static::from_slice::<ContentType>(b"\x20").map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
}

/// `context : "Signature" / "Signature1"`: a choice of text values is an enum of unit variants
/// written as their names.
#[test]
fn a_choice_of_text_values_is_an_enum_of_names() {
    let structure = SigStructure1 {
        context: cbor_static_conformance::cose::SigContext::Signature1,
        body_protected: Protected(b""),
        external_aad: b"",
        payload: b"",
    };
    assert_eq!(
        encode(&structure).expect("encodes")[1..12],
        *b"\x6aSignature1"
    );
    assert_eq!(
        cbor_static::from_slice::<EncContext>(b"\x6dEnc_Recipient"),
        Ok(EncContext::EncRecipient)
    );
}

/// `COSE_Sign1 = [ Headers, payload : bstr / nil, signature : bstr ]`: positional entries are an
/// array struct.
#[test]
fn an_array_of_named_entries_is_an_array_struct() {
    let sign1: Sign1 = cbor_static::from_slice(b"\x84\x41\xa0\xa0\x40\x41\x00").expect("decodes");
    assert_eq!(
        (sign1.protected, sign1.signature),
        (Protected(b"\xa0"), b"\x00".as_slice())
    );
}

/// `Headers = ( protected : ..., unprotected : header_map )`, a group spliced into every
/// message's array. An array struct cannot splice another, so each message declares the group's
/// two fields itself.
#[test]
fn a_group_spliced_into_an_array_is_declared_in_each_struct() {
    let signature: Signature =
        cbor_static::from_slice(b"\x83\x40\xa1\x04\x41\x31\x40").expect("decodes");
    let sign1: Sign1 =
        cbor_static::from_slice(b"\x84\x40\xa1\x04\x41\x31\xf6\x40").expect("decodes");
    assert_eq!(
        (signature.protected, &signature.unprotected),
        (sign1.protected, &sign1.unprotected)
    );
}

/// `? recipients`, `? other`, `? SuppPrivInfo`: an optional last element is an `Option`, absent
/// when `None`.
#[test]
fn an_optional_last_element_is_an_option() {
    let info: SuppPubInfo = cbor_static::from_slice(b"\x82\x18\x80\x40").expect("decodes");
    assert_eq!(info.other, None);
    assert_eq!(encode(&info).expect("encodes"), b"\x82\x18\x80\x40");
}

/// `ciphertext : bstr / nil` last in `COSE_Encrypt0`: an `Option` there would drop a written nil,
/// so the choice is its own type and keeps the element.
#[test]
fn a_nullable_last_element_keeps_its_nil() {
    #[derive(Cbor, Debug, PartialEq)]
    #[cbor(array)]
    struct OptionLast<'a> {
        protected: &'a [u8],
        ciphertext: Option<&'a [u8]>,
    }

    let written = b"\x83\x40\xa0\xf6";
    assert_eq!(
        reencode::<cbor_static_conformance::cose::Encrypt0>(written).as_deref(),
        Ok(written.as_slice())
    );
    assert_eq!(
        reencode::<OptionLast>(b"\x82\x40\xf6").as_deref(),
        Ok(b"\x81\x40".as_slice())
    );
}

/// `[ context, body_protected, ? sign_protected, external_aad, payload ]`: an optional element
/// before required ones has no array-struct form, so its two shapes are two types.
#[test]
fn an_optional_element_mid_array_is_two_types() {
    let with = b"\x85\x69Signature\x40\x40\x40\x40";
    let without = b"\x84\x6aSignature1\x40\x40\x40";
    assert!(cbor_static::from_slice::<SigStructure>(with).is_ok());
    assert!(cbor_static::from_slice::<SigStructure1>(without).is_ok());
    assert!(cbor_static::from_slice::<SigStructure>(without).is_err());
}

/// `COSE_recipient = [ Headers, ciphertext, ? recipients : [+ COSE_recipient] ]` refers to
/// itself, which a derived type cannot: recipients are typed to a fixed depth, raw below it.
#[test]
fn a_recursive_rule_is_typed_to_a_fixed_depth() {
    let three_deep = b"\x84\x40\xa0\xf6\x81\x84\x40\xa0\xf6\x81\x83\x40\xa0\xf6";
    let outer: Recipient<Recipient> = cbor_static::from_slice(three_deep).expect("decodes");
    let deepest: Vec<RawCbor> = outer
        .recipients
        .iter()
        .flatten()
        .flat_map(|inner| inner.recipients.iter().flatten().copied())
        .collect();
    assert_eq!(
        deepest.iter().map(RawCbor::as_bytes).collect::<Vec<_>>(),
        [b"\x83\x40\xa0\xf6".as_slice()]
    );
    assert_eq!(encode(&outer).expect("encodes"), three_deep);
}

/// `[+ COSE_Signature]`, `[+ COSE_recipient]`, `[+ label]`: a bounded vector holds them, and
/// "at least one" is a check after decoding.
#[test]
fn one_or_more_is_a_check_after_decoding() {
    let none: Sign = cbor_static::from_slice(b"\x84\x40\xa0\xf6\x80").expect("decodes");
    assert!(none.signatures.is_empty());
    let crit: Headers = cbor_static::from_slice(b"\xa1\x02\x80").expect("decodes");
    assert_eq!(crit.crit.map(|labels| labels.is_empty()), Some(true));
}

/// `header_map = { Generic_Headers, * label => values }`: known labels are fields with integer
/// keys, and the wildcard is `other`; or the whole map is an order-keeping map of raw values.
#[test]
fn a_map_with_known_and_wildcard_entries_is_fields_and_other() {
    let written = b"\xa2\x01\x26\x20\x01";
    let typed: Headers = cbor_static::from_slice(written).expect("decodes");
    assert_eq!(typed.alg, Some(Label::Int(-7)));
    assert_eq!(
        typed
            .other
            .iter()
            .map(|RawEntry { key, value }| (key.as_bytes(), value.as_bytes()))
            .collect::<Vec<_>>(),
        [(b"\x20".as_slice(), b"\x01".as_slice())]
    );
    assert_eq!(
        reencode::<HeaderMap>(written).as_deref(),
        Ok(written.as_slice())
    );
}

/// `Generic_Headers` spliced into `header_map`: a group spliced into a map is a flattened
/// struct.
#[test]
fn a_group_spliced_into_a_map_is_a_flattened_struct() {
    #[derive(Cbor, Debug, PartialEq)]
    struct Generic {
        #[cbor(key = 1)]
        alg: i32,
    }
    #[derive(Cbor, Debug, PartialEq)]
    struct Map {
        #[serde(flatten)]
        generic: Generic,
        #[cbor(key = 4)]
        kid: u8,
    }
    let written = b"\xa2\x01\x26\x04\x01";
    assert_eq!(
        cbor_static::from_slice::<Map>(written),
        Ok(Map {
            generic: Generic { alg: -7 },
            kid: 1
        })
    );
    assert_eq!(reencode::<Map>(written).as_deref(), Ok(written.as_slice()));
}

/// `? ( 5 => bstr // 6 => bstr )`, IV or Partial IV but not both: two `Option`s, and "not both"
/// is a check after decoding.
#[test]
fn a_group_choice_is_options_and_a_check_after_decoding() {
    let both: Headers = cbor_static::from_slice(b"\xa2\x05\x40\x06\x40").expect("decodes");
    assert!(both.iv.is_some() && both.partial_iv.is_some());
}

/// `empty_or_serialized_map = bstr .cbor header_map / bstr .size 0`: there is no untagged
/// `.cbor` type, so the bytes are kept and decoded a second time.
#[test]
fn a_serialized_map_in_a_byte_string_is_decoded_a_second_time() {
    let protected: Protected = cbor_static::from_slice(b"\x43\xa1\x01\x26").expect("decodes");
    assert_eq!(protected.headers().map(|map| map.len()), Ok(1));
    assert_eq!(Protected(b"").headers().map(|map| map.len()), Ok(0));
    assert!(Protected(b"\x01").headers().is_err());
}

/// `COSE_Key = { 1 => tstr / int, ? 2 => bstr, ..., * label => values }`: required and optional
/// integer-keyed fields with a wildcard.
#[test]
fn a_map_of_required_optional_and_wildcard_entries() {
    let key: Key = cbor_static::from_slice(b"\xa2\x01\x04\x20\x01").expect("decodes");
    assert_eq!(
        (key.kty, key.kid, key.parameters.len()),
        (Label::Int(4), None, 1)
    );
    assert_eq!(
        cbor_static::from_slice::<Key>(b"\xa1\x02\x40").map_err(|e| e.kind),
        Err(ErrorKind::MissingField)
    );
}

/// `COSE_KeySet = [+ COSE_Key]`: a homogeneous array is a bounded vector.
#[test]
fn a_homogeneous_array_is_a_bounded_vector() {
    let set: KeySet = cbor_static::from_slice(b"\x81\xa1\x01\x04").expect("decodes");
    assert_eq!(set.0.len(), 1);
}

/// `int`: CBOR integers span -2^64 to 2^64-1; a label is an `i64`, so the rest are out of range.
#[test]
fn an_integer_label_outside_i64_is_out_of_range() {
    assert_eq!(
        cbor_static::from_slice::<Label>(b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff")
            .map_err(|e| e.kind),
        Err(ErrorKind::OutOfRange)
    );
}

/// `values = any`: an unconstrained item is kept raw and written back verbatim.
#[test]
fn any_is_a_raw_item() {
    let map: HeaderMap = cbor_static::from_slice(b"\xa1\x21\x82\x19\x01\x00\x80").expect("decodes");
    assert_eq!(
        map.get(&Label::Int(-2)).map(RawCbor::as_bytes),
        Some(b"\x82\x19\x01\x00\x80".as_slice())
    );
}
