//! Zephyr's `MCUmgr` firmware answers in indefinite-length maps, which cbor-static decodes but never
//! writes. Each array decodes, and its definite re-encoding decodes to the same value.

use cbor_static::ErrorKind;
use cbor_static_conformance::smp::ErrorV1;
use cbor_static_conformance::smp::os::{EchoWriteRequest, EchoWriteResponse};

use crate::common::encode;

/// The value the indefinite-length `firmware` map decodes to, once its definite re-encoding has
/// decoded to the same value.
macro_rules! as_data {
    ($type:ty, $firmware:expr) => {{
        let firmware = $firmware;
        let value = cbor_static::from_slice::<$type>(firmware).expect("the firmware's map decodes");
        let definite = encode(&value).expect("the value encodes");
        assert_eq!(
            (firmware.first(), firmware.last()),
            (Some(&0xbf), Some(&0xff))
        );
        assert_ne!(definite, firmware);
        assert_eq!(cbor_static::from_slice::<$type>(&definite), Ok(value));
        value
    }};
}

#[test]
fn echo_command() {
    as_data!(EchoWriteRequest, twin!("zephyr/os_mgmt_echo/command"));
}

#[test]
fn echo_response_echoes_the_command() {
    assert_eq!(
        as_data!(
            EchoWriteResponse,
            twin!("zephyr/os_mgmt_echo/expected_response")
        )
        .r,
        as_data!(EchoWriteRequest, twin!("zephyr/os_mgmt_echo/command")).d
    );
}

#[test]
fn echo_command_without_data_is_missing_its_string() {
    let firmware = twin!("zephyr/os_mgmt_echo/command_no_data");
    assert_eq!(firmware, b"\xbf\xff");
    assert_eq!(
        cbor_static::from_slice::<EchoWriteRequest>(firmware).map_err(|e| e.kind),
        Err(ErrorKind::MissingField)
    );
}

#[test]
fn echo_response_without_data_is_an_error() {
    let firmware = twin!("zephyr/os_mgmt_echo/expected_response_no_data");
    assert_eq!(
        cbor_static::from_slice::<EchoWriteResponse>(firmware).map_err(|e| e.kind),
        Err(ErrorKind::UnknownField)
    );
    as_data!(ErrorV1, firmware);
}

#[test]
fn info_command() {
    use cbor_static_conformance::smp::os::OSApplicationInfoReadRequest;

    as_data!(
        OSApplicationInfoReadRequest,
        twin!("zephyr/os_mgmt_info/command")
    );
}

#[test]
fn info_response() {
    use cbor_static_conformance::smp::os::OSApplicationInfoReadResponse;

    as_data!(
        OSApplicationInfoReadResponse,
        twin!("zephyr/os_mgmt_info/expected_response")
    );
}

#[test]
fn a_legacy_frame_is_unsupported_as_too_old() {
    assert_eq!(
        as_data!(ErrorV1, twin!("zephyr/smp_version/response_old")),
        ErrorV1 { rc: 12, rsn: None }
    );
}

#[test]
fn a_legacy_frame_with_the_original_protocol_is_invalid() {
    assert_eq!(
        as_data!(
            ErrorV1,
            twin!("zephyr/smp_version/response_old_original_protocol")
        ),
        ErrorV1 { rc: 3, rsn: None }
    );
}

#[test]
fn a_current_frame_answers_with_a_group_error() {
    use cbor_static_conformance::smp::{ErrorV2, error};

    assert_eq!(
        as_data!(ErrorV2, twin!("zephyr/smp_version/response_current")),
        ErrorV2 {
            err: error::Err { group: 0, rc: 2 }
        }
    );
}

#[test]
fn a_frame_too_new_is_unsupported_as_too_new() {
    assert_eq!(
        as_data!(ErrorV1, twin!("zephyr/smp_version/response_new")),
        ErrorV1 { rc: 13, rsn: None }
    );
}
