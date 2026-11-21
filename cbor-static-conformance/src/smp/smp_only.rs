//! Shapes JPHutchins/smp writes and Zephyr's firmware does not, under smp's class names. smp's
//! binary regression records or smp's own encoder produce them, so they decode here; the
//! firmware's shapes are in [`os`](super::os).

use cbor_static::Cbor;
use heapless::LinearMap;

use super::os::MEMORY_POOLS;

/// Memory pool statistics' response with the pools at the root, where smp's class reads them;
/// Zephyr wraps them in `"mpools"` (JPHutchins/smp#77).
///
/// ```
/// use cbor_static_conformance::smp::smp_only::MemoryPoolStatisticsReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/memory_pool_statistics_read_response/1.cbor"));
/// let response: MemoryPoolStatisticsReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!(response.0.get("mem_pool_2").map(|pool| pool.nfree), Some(7));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[cbor(length_first)]
pub struct MemoryPoolStatisticsReadResponse<'a>(
    #[serde(borrow)] pub LinearMap<&'a str, MemoryPoolStatistics, MEMORY_POOLS>,
);

/// One memory pool's statistics with the block size required, as `blksize`; Zephyr spells it
/// `blksiz` and omits it by default (JPHutchins/smp#78).
///
/// ```
/// use cbor_static_conformance::smp::smp_only::MemoryPoolStatistics;
///
/// let pool = MemoryPoolStatistics { blksize: 1, nblks: 2, nfree: 3, min: 4 };
/// let mut out = [0u8; 32];
/// let len = cbor_static::to_slice(&pool, &mut out)?;
/// assert_eq!(cbor_static::from_slice::<MemoryPoolStatistics>(&out[..len])?, pool);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct MemoryPoolStatistics {
    pub blksize: i64,
    pub nblks: i64,
    pub nfree: i64,
    pub min: i64,
}

/// Reset with `force` as an integer, as smp sends it; Zephyr decodes a bool (JPHutchins/smp#79).
/// Its vector is smp's own encoding of `ResetWriteRequest(force=1)`, not a record.
///
/// ```
/// use cbor_static_conformance::smp::smp_only::ResetWriteRequest;
///
/// let built = include_bytes!(concat!(env!("SMP_VECTORS"), "/smp_only/reset_write_request.cbor"));
/// assert_eq!(cbor_static::from_slice::<ResetWriteRequest>(built)?.force, Some(1));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ResetWriteRequest {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub force: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub boot_mode: Option<u32>,
}
