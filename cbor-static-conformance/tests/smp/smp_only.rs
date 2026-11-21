//! Each shape only smp writes, pinned by name to the smp issue that records it: its vector decodes
//! as the `smp_only` type, and not as the firmware-shaped type.

use cbor_static::ErrorKind;
use cbor_static_conformance::smp::smp_only;

/// JPHutchins/smp#77 and #78: smp's record keeps the pools at the root, each with a `blksize`.
#[test]
fn memory_pools_at_the_root_are_smp_only() {
    use cbor_static_conformance::smp::os::MemoryPoolStatisticsReadResponse;

    let written = twin!("os/memory_pool_statistics_read_response/1");
    assert!(cbor_static::from_slice::<smp_only::MemoryPoolStatisticsReadResponse>(written).is_ok());
    assert_eq!(
        cbor_static::from_slice::<MemoryPoolStatisticsReadResponse>(written).map_err(|e| e.kind),
        Err(ErrorKind::UnknownField)
    );
}

/// JPHutchins/smp#79: smp sends `force` as an integer, which Zephyr does not decode as a bool.
#[test]
fn an_integer_force_is_smp_only() {
    use cbor_static_conformance::smp::os::ResetWriteRequest;

    let built = twin!("smp_only/reset_write_request");
    assert_eq!(
        cbor_static::from_slice::<smp_only::ResetWriteRequest>(built).map(|request| request.force),
        Ok(Some(1))
    );
    assert_eq!(
        cbor_static::from_slice::<ResetWriteRequest>(built).map_err(|e| e.kind),
        Err(ErrorKind::TypeMismatch)
    );
}
