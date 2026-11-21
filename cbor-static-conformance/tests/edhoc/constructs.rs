//! Every CDDL construct RFC 9528 Appendix C.2 uses, one test each: how the EDHOC types express it,
//! or, where the derive cannot yet, the nearest form and the check it needs after decoding.

use cbor_static::ErrorKind;
use cbor_static_conformance::edhoc::{ConnectionId, Sequence, Suites};

/// `message_1 = ( METHOD : int, SUITES_I : suites, ... )`: a group sent as a CBOR sequence, not
/// an array. The derive writes one item, so the nearest form is a `Sequence` that chains
/// `from_slice_partial` over its items (#64).
#[test]
fn a_group_sent_as_a_sequence_chains_its_items() {
    use cbor_static_conformance::edhoc::Message1;

    assert_eq!(
        Message1::from_sequence(b"\x03\x02\x41\x00\x2d").map(|message| (
            message.method,
            message.suites_i,
            message.g_x,
            message.c_i
        )),
        Ok((
            3,
            Suites::One(2),
            b"\x00".as_slice(),
            ConnectionId::Int(-14)
        ))
    );
    assert_eq!(
        Message1::from_sequence(b"\x84\x03\x02\x41\x00\x2d").map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
}

/// `message_2 = ( G_Y_CIPHERTEXT_2 : bstr )`: a one-item group is the item, a newtype struct.
#[test]
fn a_one_item_group_is_a_newtype() {
    use crate::common::reencode;
    use cbor_static_conformance::edhoc::Message2;

    assert_eq!(
        reencode::<Message2>(b"\x41\x00").as_deref(),
        Ok(b"\x41\x00".as_slice())
    );
    assert_eq!(
        cbor_static::from_slice::<Message2>(b"\x41\x00\x41\x00").map_err(|e| e.kind),
        Err(ErrorKind::TrailingInput)
    );
}

/// `suites = [ 2* int ] / int`: alternatives by major type are an untagged enum; the array's
/// minimum of two is a check after decoding (#64).
#[test]
fn suites_are_an_int_or_an_array_whose_minimum_is_checked_after_decoding() {
    let Ok(Suites::Many(one)) = cbor_static::from_slice::<Suites>(b"\x81\x02") else {
        panic!("an array of one decodes")
    };
    assert!(one.len() < 2);
}

/// `C_I : bstr / -24..23`: an untagged enum; the range is a check after decoding (#64).
#[test]
fn a_connection_identifier_range_is_checked_after_decoding() {
    let Ok(ConnectionId::Int(outside)) = cbor_static::from_slice::<ConnectionId>(b"\x18\x18")
    else {
        panic!("24 decodes")
    };
    assert!(!(-24..=23).contains(&outside));
}

/// `ID_CRED_R : header_map / bstr / -24..23`: three alternatives by major type, the map as
/// written.
#[test]
fn an_identifier_is_a_map_bytes_or_an_int() {
    use cbor_static_conformance::cose::Label;
    use cbor_static_conformance::edhoc::IdCred;

    assert!(matches!(
        cbor_static::from_slice::<IdCred>(b"\xa1\x04\x41\x32"),
        Ok(IdCred::Map(map)) if map.keys().eq([Label::Int(4)].iter())
    ));
    assert_eq!(
        [b"\x41\x32".as_slice(), b"\x32"].map(cbor_static::from_slice::<IdCred>),
        [Ok(IdCred::Bytes(b"\x32")), Ok(IdCred::Int(-19))]
    );
}

/// `ead = ( ead_label : int, ? ead_value : bstr )`: an optional item inside a sequence is present
/// when the next item's head says it is.
#[test]
fn an_optional_item_in_a_sequence_is_read_by_its_head() {
    use cbor_static_conformance::edhoc::Ead;

    assert_eq!(
        [b"\x00\x41\x00".as_slice(), b"\x00\x01"]
            .map(|input| Ead::decode_front(input).map(|(ead, rest)| (ead, rest.len()))),
        [
            Ok((
                Ead {
                    label: 0,
                    value: Some(b"\x00")
                },
                0
            )),
            Ok((
                Ead {
                    label: 0,
                    value: None
                },
                1
            ))
        ]
    );
}

/// `EAD_1 = (+ ead)` in `? EAD_1`: one or more, or none at the end of the message. Absent and
/// empty are the same bytes, so the minimum is not a decoding rule.
#[test]
fn trailing_ead_items_are_zero_or_more() {
    use cbor_static_conformance::edhoc::Eads;

    assert_eq!(
        [b"".as_slice(), b"\x00", b"\x00\x20\x41\x01"]
            .map(|input| Eads::from_sequence(input).map(|eads| eads.len())),
        [Ok(0), Ok(1), Ok(2)]
    );
}

/// `ERR_INFO : any`: a raw item.
#[test]
fn any_is_a_raw_item() {
    use cbor_static_conformance::edhoc::ErrorMessage;

    let error = ErrorMessage::from_sequence(b"\x01\x63abc").expect("decodes");
    assert_eq!(
        (error.err_code, error.err_info.as_bytes()),
        (1, b"\x63abc".as_slice())
    );
}

/// `length : uint`: an unsigned integer is out of range for a negative one.
#[test]
fn a_uint_rejects_a_negative_integer() {
    use cbor_static_conformance::edhoc::Info;

    assert_eq!(
        Info::from_sequence(b"\x00\x40\x20").map_err(|e| e.kind),
        Err(ErrorKind::OutOfRange)
    );
}

/// RFC 9528 §5.3.2's `<< TH_2, CRED_R, ? EAD_2 >>`: a sequence wrapped in a byte string, without
/// tag 24. The nearest form is the bytes and a second decode as a sequence (#64).
#[test]
fn a_wrapped_sequence_is_decoded_a_second_time() {
    use cbor_static_conformance::cose::SigStructure1;
    use cbor_static_conformance::edhoc::Th2Input;

    assert_eq!(
        cbor_static::from_slice::<SigStructure1>(b"\x84\x6aSignature1\x40\x44\x41\x01\x41\x02\x40")
            .and_then(|signed| Th2Input::from_sequence(signed.external_aad)),
        Ok(Th2Input {
            g_y: b"\x01",
            hash_message_1: b"\x02"
        })
    );
}
