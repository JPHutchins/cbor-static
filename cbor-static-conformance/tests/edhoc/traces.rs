//! The traces' items against each other: each composite item is the items RFC 9528 builds it from,
//! byte for byte, and the identifiers agree wherever they reappear. One case per trace and item.

use cbor_static_conformance::cose::Label;
use cbor_static_conformance::edhoc::{Cred, Plaintext2, Sequence};

use crate::common::encode;

/// A byte string item's content.
fn content(item: &[u8]) -> &[u8] {
    cbor_static::from_slice(item).expect("a byte string")
}

/// RFC 9528 §5.3.2: `TH_2 = H( G_Y, H(message_1) )`, and `TH_3 = H( TH_2, PLAINTEXT_2, CRED_R )`
/// and `TH_4` likewise; `context_2 = << C_R, ID_CRED_R, TH_2, CRED_R, ? EAD_2 >>`, and
/// `context_3` without the connection identifier.
fn is_its_parts(composite: &[u8], parts: &[&[u8]]) {
    assert_eq!(composite, parts.concat());
}

/// RFC 9528 §5.3.1: the byte string of `message_2` starts with `G_Y`.
fn message_2_starts_with_g_y(message_2: &[u8], g_y: &[u8]) {
    use cbor_static_conformance::edhoc::Message2;

    assert!(
        cbor_static::from_slice::<Message2>(message_2)
            .expect("decodes")
            .0
            .starts_with(content(g_y))
    );
}

/// RFC 9528 §5.4.3: `A_3 = [ "Encrypt0", h'', TH_3 ]`, and `A_4` with `TH_4`.
fn associated_data_is(structure: &[u8], th: &[u8]) {
    use cbor_static_conformance::cose::{EncContext, EncStructure};

    assert_eq!(
        cbor_static::from_slice::<EncStructure>(structure).map(|structure| (
            structure.context,
            structure.protected.0,
            structure.external_aad
        )),
        Ok((EncContext::Encrypt0, b"".as_slice(), content(th)))
    );
}

/// RFC 9528 §5.3.2: the message to be signed is COSE's `Sig_structure` over `<< ID_CRED_R >>`,
/// `<< TH_2, CRED_R, ? EAD_2 >>` and `MAC_2`; §5.4.2 likewise for message 3.
fn signs(signed: &[u8], id_cred: &[u8], transcript: &[&[u8]], mac: &[u8]) {
    use cbor_static_conformance::cose::SigStructure1;

    assert_eq!(
        cbor_static::from_slice::<SigStructure1>(signed).map(|structure| (
            structure.body_protected.0,
            structure.external_aad.to_vec(),
            structure.payload
        )),
        Ok((id_cred, transcript.concat(), content(mac)))
    );
}

/// The kid in a `{4: kid}` identifier map.
fn kid(id_cred: &[u8]) -> &[u8] {
    use cbor_static_conformance::cose::HeaderMap;

    cbor_static::from_slice::<HeaderMap>(id_cred)
        .ok()
        .and_then(|map| map.get(&Label::Int(4)).map(cbor_static::RawCbor::as_bytes))
        .and_then(|kid| cbor_static::from_slice(kid).ok())
        .expect("a kid")
}

/// RFC 9528 §3.5.3.2: a plaintext carries a lone one-byte `kid` as the integer that byte encodes.
fn compacts_its_kid(compact: &impl cbor_static::Encode, id_cred: &[u8]) {
    assert_eq!(encode(compact).as_deref(), Ok(kid(id_cred)));
}

/// RFC 9528 §3.5.2: a CCS's confirmation claim (`cnf`, 8) holds a `COSE_Key` (1) with the kid
/// that identifies the credential.
fn confirms_its_kid(cred: &[u8], id_cred: &[u8]) {
    use cbor_static_conformance::cose::{HeaderMap, Key};

    let Ok(Cred::Ccs(ccs)) = cbor_static::from_slice::<Cred>(cred) else {
        panic!("a CCS")
    };
    assert_eq!(
        ccs.other
            .iter()
            .find(|claim| claim.key.as_bytes() == b"\x08")
            .and_then(|cnf| cbor_static::from_slice::<HeaderMap>(cnf.value.as_bytes()).ok())
            .and_then(|methods| methods
                .get(&Label::Int(1))
                .and_then(|key| cbor_static::from_slice::<Key>(key.as_bytes()).ok()))
            .and_then(|key| key.kid),
        Some(kid(id_cred))
    );
}

#[test]
fn section_2_th_2_input_is_g_y_and_the_hash_of_message_1() {
    is_its_parts(
        rfc9529!(section_2_2_input_to_calculate_th_2),
        &[rfc9529!(section_2_2_g_y), rfc9529!(section_2_2_h_message_1)],
    );
}

#[test]
fn section_3_th_2_input_is_g_y_and_the_hash_of_message_1() {
    is_its_parts(
        rfc9529!(section_3_4_input_to_calculate_th_2),
        &[rfc9529!(section_3_4_g_y), rfc9529!(section_3_4_h_message_1)],
    );
}

#[test]
fn section_2_th_3_input_is_th_2_plaintext_2_and_cred_r() {
    is_its_parts(
        rfc9529!(section_2_3_input_to_calculate_th_3),
        &[
            rfc9529!(section_2_2_th_2),
            rfc9529!(section_2_2_plaintext_2),
            rfc9529!(section_2_2_cred_r),
        ],
    );
}

#[test]
fn section_2_th_4_input_is_th_3_plaintext_3_and_cred_i() {
    is_its_parts(
        rfc9529!(section_2_3_input_to_calculate_th_4),
        &[
            rfc9529!(section_2_3_th_3),
            rfc9529!(section_2_3_plaintext_3),
            rfc9529!(section_2_3_cred_i),
        ],
    );
}

#[test]
fn section_3_th_3_input_is_th_2_plaintext_2_and_cred_r() {
    is_its_parts(
        rfc9529!(section_3_5_input_to_calculate_th_3),
        &[
            rfc9529!(section_3_4_th_2),
            rfc9529!(section_3_4_plaintext_2),
            rfc9529!(section_3_4_cred_r),
        ],
    );
}

#[test]
fn section_3_th_4_input_is_th_3_plaintext_3_and_cred_i() {
    is_its_parts(
        rfc9529!(section_3_5_input_to_calculate_th_4),
        &[
            rfc9529!(section_3_5_th_3),
            rfc9529!(section_3_5_plaintext_3),
            rfc9529!(section_3_5_cred_i),
        ],
    );
}

#[test]
fn section_2_context_2_is_its_parts() {
    is_its_parts(
        rfc9529!(section_2_2_context_2),
        &[
            rfc9529!(section_2_2_c_r),
            rfc9529!(section_2_2_id_cred_r),
            rfc9529!(section_2_2_th_2),
            rfc9529!(section_2_2_cred_r),
            rfc9529!(section_2_2_ead_2),
        ],
    );
}

#[test]
fn section_2_context_3_is_its_parts() {
    is_its_parts(
        rfc9529!(section_2_3_context_3),
        &[
            rfc9529!(section_2_3_id_cred_i),
            rfc9529!(section_2_3_th_3),
            rfc9529!(section_2_3_cred_i),
            rfc9529!(section_2_3_ead_3),
        ],
    );
}

#[test]
fn section_3_context_2_is_its_parts() {
    is_its_parts(
        rfc9529!(section_3_4_context_2),
        &[
            rfc9529!(section_3_4_c_r),
            rfc9529!(section_3_4_id_cred_r),
            rfc9529!(section_3_4_th_2),
            rfc9529!(section_3_4_cred_r),
            rfc9529!(section_3_4_ead_2),
        ],
    );
}

#[test]
fn section_3_context_3_is_its_parts() {
    is_its_parts(
        rfc9529!(section_3_5_context_3),
        &[
            rfc9529!(section_3_5_id_cred_i),
            rfc9529!(section_3_5_th_3),
            rfc9529!(section_3_5_cred_i),
            rfc9529!(section_3_5_ead_3),
        ],
    );
}

#[test]
fn section_2_message_2_starts_with_g_y() {
    message_2_starts_with_g_y(rfc9529!(section_2_2_message_2), rfc9529!(section_2_2_g_y));
}

#[test]
fn section_3_message_2_starts_with_g_y() {
    message_2_starts_with_g_y(rfc9529!(section_3_4_message_2), rfc9529!(section_3_4_g_y));
}

#[test]
fn section_2_a_3_is_over_th_3() {
    associated_data_is(rfc9529!(section_2_3_a_3), rfc9529!(section_2_3_th_3));
}

#[test]
fn section_2_a_4_is_over_th_4() {
    associated_data_is(rfc9529!(section_2_4_a_4), rfc9529!(section_2_3_th_4));
}

#[test]
fn section_3_a_3_is_over_th_3() {
    associated_data_is(rfc9529!(section_3_5_a_3), rfc9529!(section_3_5_th_3));
}

#[test]
fn section_3_a_4_is_over_th_4() {
    associated_data_is(rfc9529!(section_3_6_a_4), rfc9529!(section_3_5_th_4));
}

#[test]
fn section_2_message_2_signs_its_identifier_transcript_credential_and_mac() {
    signs(
        rfc9529!(section_2_2_message_to_be_signed_in_message_2),
        rfc9529!(section_2_2_id_cred_r),
        &[
            rfc9529!(section_2_2_th_2),
            rfc9529!(section_2_2_cred_r),
            rfc9529!(section_2_2_ead_2),
        ],
        rfc9529!(section_2_2_mac_2),
    );
}

#[test]
fn section_2_message_3_signs_its_identifier_transcript_credential_and_mac() {
    signs(
        rfc9529!(section_2_3_message_to_be_signed_in_message_3),
        rfc9529!(section_2_3_id_cred_i),
        &[
            rfc9529!(section_2_3_th_3),
            rfc9529!(section_2_3_cred_i),
            rfc9529!(section_2_3_ead_3),
        ],
        rfc9529!(section_2_3_mac_3),
    );
}

/// Section 2 identifies its X.509 credentials by `x5t` (label 34), a map that `PLAINTEXT_2` carries
/// whole.
#[test]
fn section_2_carries_its_identifier_map_whole() {
    use cbor_static_conformance::edhoc::IdCred;

    assert!(matches!(
        Plaintext2::from_sequence(rfc9529!(section_2_2_plaintext_2)).map(|plaintext| plaintext.id_cred_r),
        Ok(IdCred::Map(map))
            if map.keys().eq([Label::Int(34)].iter())
                && encode(&map).as_deref() == Ok(rfc9529!(section_2_2_id_cred_r))
    ));
    assert!(matches!(
        cbor_static::from_slice::<Cred>(rfc9529!(section_2_2_cred_r)),
        Ok(Cred::Certificate(_))
    ));
}

#[test]
fn section_3_plaintext_2_compacts_its_kid() {
    compacts_its_kid(
        &Plaintext2::from_sequence(rfc9529!(section_3_4_plaintext_2))
            .expect("decodes")
            .id_cred_r,
        rfc9529!(section_3_4_id_cred_r),
    );
}

#[test]
fn section_3_plaintext_3_compacts_its_kid() {
    use cbor_static_conformance::edhoc::Plaintext3;

    compacts_its_kid(
        &Plaintext3::from_sequence(rfc9529!(section_3_5_plaintext_3))
            .expect("decodes")
            .id_cred_i,
        rfc9529!(section_3_5_id_cred_i),
    );
}

#[test]
fn section_3_cred_r_confirms_its_kid() {
    confirms_its_kid(
        rfc9529!(section_3_4_cred_r),
        rfc9529!(section_3_4_id_cred_r),
    );
}

#[test]
fn section_3_cred_i_confirms_its_kid() {
    confirms_its_kid(
        rfc9529!(section_3_5_cred_i),
        rfc9529!(section_3_5_id_cred_i),
    );
}

/// Section 3's Initiator first offers suite 6, which the Responder refuses with error code 2 and
/// the suite it supports; the Initiator's second `message_1` offers both, that suite last.
#[test]
fn section_3_negotiates_its_suite_through_an_error() {
    use cbor_static_conformance::edhoc::{ErrorMessage, Message1, Suites};

    let error = ErrorMessage::from_sequence(rfc9529!(section_3_2_error)).expect("decodes");
    let suites_i = |message| Message1::from_sequence(message).map(|message| message.suites_i);
    let Ok(Suites::One(supported)) = cbor_static::from_slice::<Suites>(error.err_info.as_bytes())
    else {
        panic!("one suite")
    };
    let Ok(Suites::Many(offered)) = suites_i(rfc9529!(section_3_3_message_1)) else {
        panic!("several suites")
    };
    assert_eq!(
        (
            error.err_code,
            suites_i(rfc9529!(section_3_1_message_1)),
            offered.last()
        ),
        (2, Ok(Suites::One(6)), Some(&supported))
    );
}
