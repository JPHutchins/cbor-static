#![no_main]

use cbor_static_conformance::smp::{Empty, error, os, smp_only};

libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    cbor_static_fuzz::typed!(
        input,
        [
            Empty,
            error::ErrorV1<'_>,
            error::ErrorV2,
            error::Err,
            os::EchoWriteRequest<'_>,
            os::EchoWriteResponse<'_>,
            os::ResetWriteRequest,
            os::TaskStatisticsReadResponse<'_>,
            os::TaskStatisticsZephyr,
            os::TaskStatistics,
            os::MemoryPoolStatisticsReadResponse<'_>,
            os::MemoryPoolStatistics,
            os::DateTimeReadResponse<'_>,
            os::MCUMgrParametersReadResponse,
            os::OSApplicationInfoReadRequest<'_>,
            os::OSApplicationInfoReadResponse<'_>,
            os::BootloaderInformationReadResponse<'_>,
            os::MCUbootModeReadRequest<'_>,
            os::MCUbootModeReadResponse,
            smp_only::MemoryPoolStatisticsReadResponse<'_>,
            smp_only::MemoryPoolStatistics,
            smp_only::ResetWriteRequest,
        ]
    );
});
