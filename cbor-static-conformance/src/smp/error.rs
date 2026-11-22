use cbor_static::Cbor;

/// SMP version 1's error response, which any command can answer with: the `MCUmgr` error code, and
/// optionally why. It replaces the command's own response, and both are maps, so the application
/// decodes it as this type by context.
///
/// ```
/// use cbor_static_conformance::smp::ErrorV1;
///
/// let firmware = include_bytes!(concat!(env!("SMP_VECTORS"), "/zephyr/os_mgmt_echo/expected_response_no_data.cbor"));
/// assert_eq!(cbor_static::from_slice::<ErrorV1>(firmware)?, ErrorV1 { rc: 3, rsn: None });
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ErrorV1<'a> {
    pub rc: i64,
    #[cbor(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub rsn: Option<&'a str>,
}

/// SMP version 2's error response: the group that failed, and its own error code.
///
/// ```
/// use cbor_static_conformance::smp::{ErrorV2, error};
///
/// let firmware = include_bytes!(concat!(env!("SMP_VECTORS"), "/zephyr/smp_version/response_current.cbor"));
/// assert_eq!(cbor_static::from_slice::<ErrorV2>(firmware)?, ErrorV2 { err: error::Err { group: 0, rc: 2 } });
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ErrorV2 {
    pub err: Err,
}

/// The group and the group's error code in an [`ErrorV2`].
///
/// ```
/// use cbor_static_conformance::smp::error;
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&error::Err { group: 1, rc: 5 }, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x62rc\x05\x65group\x01");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct Err {
    pub group: i64,
    pub rc: i64,
}
