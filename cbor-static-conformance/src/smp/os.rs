use cbor_static::Cbor;
use heapless::LinearMap;

/// The most tasks a [`TaskStatisticsReadResponse`] lists: the default of Zephyr's
/// `CONFIG_MCUMGR_GRP_OS_TASKSTAT_MAX_NUM_THREADS`, which Kconfig calls a predicted maximum, not a
/// bound (`subsys/mgmt/mcumgr/grp/os_mgmt/Kconfig`). A response with more decodes as
/// `OutOfRange`.
pub const TASKS: usize = 50;

/// The most memory pools a [`MemoryPoolStatisticsReadResponse`] lists, a cap of this crate's own:
/// Zephyr lists every heap `sys_heap_array_get` holds (`os_mgmt.c:390-394`), and
/// `CONFIG_SYS_HEAP_ARRAY_SIZE` has no range. A response with more decodes as `OutOfRange`.
pub const MEMORY_POOLS: usize = 8;

/// Echo (command 0): the string to echo.
///
/// ```
/// use cbor_static_conformance::smp::os::EchoWriteRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/echo_write_request/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<EchoWriteRequest>(written)?.d, "Hello world!");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct EchoWriteRequest<'a> {
    #[serde(borrow)]
    pub d: &'a str,
}

/// Echo's response: the string echoed.
///
/// ```
/// use cbor_static_conformance::smp::os::EchoWriteResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/echo_write_response/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<EchoWriteResponse>(written)?.r, "Hi!");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct EchoWriteResponse<'a> {
    #[serde(borrow)]
    pub r: &'a str,
}

/// Reset (command 5): an optional forced reset and boot mode. Zephyr decodes `force` as a bool
/// (`os_mgmt.c:481`); smp sends an integer, the shape [`smp_only::ResetWriteRequest`] keeps
/// (JPHutchins/smp#79).
///
/// [`smp_only::ResetWriteRequest`]: super::smp_only::ResetWriteRequest
///
/// ```
/// use cbor_static_conformance::smp::os::ResetWriteRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/reset_write_request/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<ResetWriteRequest>(written)?, ResetWriteRequest::default());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ResetWriteRequest {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub boot_mode: Option<u32>,
}

/// Task statistics' response (command 2): each task's statistics, keyed by its name. Zephyr's
/// names can repeat: by default, without `CONFIG_THREAD_NAME`, a task is named by its priority
/// (`os_mgmt.c:221-233`), and with it, unnamed or truncated names can match; only the thread
/// index is unique (zephyrproject-rtos/zephyr#126661). A map whose keys repeat is not valid CBOR
/// (RFC 8949 §5.6), so such a response decodes as `DuplicateKey`, where smp keeps the last.
///
/// ```
/// use cbor_static_conformance::smp::os::TaskStatisticsReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/task_statistics_read_response/1.cbor"));
/// let response: TaskStatisticsReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!(response.tasks.get("task_2").map(|task| task.prio), Some(8));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct TaskStatisticsReadResponse<'a> {
    #[serde(borrow)]
    pub tasks: LinearMap<&'a str, TaskStatisticsZephyr, TASKS>,
}

/// One task's statistics as Zephyr writes them: the priority, signed by default
/// (`os_mgmt.c:308-316`; Zephyr's docs say unsigned, zephyrproject-rtos/zephyr#126660), the ID
/// and the state, and the rest only as its Kconfig selects (`os_mgmt.c:258-316`). smp reads a
/// task as [`TaskStatistics`] or as this; every [`TaskStatistics`] also reads as this.
///
/// ```
/// use cbor_static_conformance::smp::os::TaskStatisticsZephyr;
///
/// let task = TaskStatisticsZephyr { prio: -1, tid: 0, state: 1, ..TaskStatisticsZephyr::default() };
/// let mut out = [0u8; 32];
/// let len = cbor_static::to_slice(&task, &mut out)?;
/// assert_eq!(&out[..len], b"\xa3\x63tid\x00\x64prio\x20\x65state\x01");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct TaskStatisticsZephyr {
    pub prio: i64,
    pub tid: i64,
    pub state: i64,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub stkuse: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub stksiz: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub cswcnt: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub last_checkin: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub next_checkin: Option<i64>,
}

/// One task's statistics, every field present.
///
/// ```
/// use cbor_static_conformance::smp::os::TaskStatistics;
///
/// let task = TaskStatistics { prio: -1, tid: 2, state: 3, ..TaskStatistics::default() };
/// let mut out = [0u8; 128];
/// let len = cbor_static::to_slice(&task, &mut out)?;
/// assert_eq!(cbor_static::from_slice::<TaskStatistics>(&out[..len])?, task);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct TaskStatistics {
    pub prio: i64,
    pub tid: i64,
    pub state: i64,
    pub stkuse: i64,
    pub stksiz: i64,
    pub cswcnt: i64,
    pub runtime: i64,
    pub last_checkin: i64,
    pub next_checkin: i64,
}

/// Memory pool statistics' response (command 3): each pool's statistics, keyed by its name,
/// under `"mpools"` (`os_mgmt.c:393`). smp's records keep the pools at the root, the shape
/// [`smp_only::MemoryPoolStatisticsReadResponse`] keeps (JPHutchins/smp#77).
///
/// [`smp_only::MemoryPoolStatisticsReadResponse`]: super::smp_only::MemoryPoolStatisticsReadResponse
///
/// ```
/// use cbor_static_conformance::smp::os::{MemoryPoolStatistics, MemoryPoolStatisticsReadResponse};
///
/// let pool = MemoryPoolStatistics { blksiz: None, nblks: 2048, nfree: 1024, min: 512 };
/// let response = MemoryPoolStatisticsReadResponse { mpools: [("0", pool)].into_iter().collect() };
/// let mut out = [0u8; 64];
/// let len = cbor_static::to_slice(&response, &mut out)?;
/// assert_eq!(&out[..9], b"\xa1\x66mpools\xa1");
/// assert_eq!(cbor_static::from_slice::<MemoryPoolStatisticsReadResponse>(&out[..len])?, response);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct MemoryPoolStatisticsReadResponse<'a> {
    #[serde(borrow)]
    pub mpools: LinearMap<&'a str, MemoryPoolStatistics, MEMORY_POOLS>,
}

/// One memory pool's statistics. Zephyr writes the block size as `blksiz`, and only when
/// `CONFIG_MCUMGR_GRP_OS_MPSTAT_ONLY_SUPPORTED_STATS`, which is on by default, is off
/// (`os_mgmt.c:421-424`). smp requires `blksize`, the shape [`smp_only::MemoryPoolStatistics`]
/// keeps (JPHutchins/smp#78).
///
/// [`smp_only::MemoryPoolStatistics`]: super::smp_only::MemoryPoolStatistics
///
/// ```
/// use cbor_static_conformance::smp::os::MemoryPoolStatistics;
///
/// let pool = MemoryPoolStatistics { blksiz: Some(1), nblks: 2, nfree: 3, min: 4 };
/// let mut out = [0u8; 32];
/// let len = cbor_static::to_slice(&pool, &mut out)?;
/// assert_eq!(cbor_static::from_slice::<MemoryPoolStatistics>(&out[..len])?, pool);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct MemoryPoolStatistics {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub blksiz: Option<i64>,
    pub nblks: i64,
    pub nfree: i64,
    pub min: i64,
}

/// Date-time's response (command 4): the time, as an ISO 8601 string.
///
/// ```
/// use cbor_static_conformance::smp::os::DateTimeReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/date_time_read_response/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<DateTimeReadResponse>(written)?.datetime, "2024-01-01T00:00:00Z");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct DateTimeReadResponse<'a> {
    #[serde(borrow)]
    pub datetime: &'a str,
}

/// `MCUmgr` parameters' response (command 6): the server's buffer size and count.
///
/// ```
/// use cbor_static_conformance::smp::os::MCUMgrParametersReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/mcu_mgr_parameters_read_response/1.cbor"));
/// let response: MCUMgrParametersReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!((response.buf_size, response.buf_count), (1, 2));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct MCUMgrParametersReadResponse {
    pub buf_size: i64,
    pub buf_count: i64,
}

/// OS/application info (command 7): the format letters requested.
///
/// ```
/// use cbor_static_conformance::smp::os::OSApplicationInfoReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/os_application_info_read_request/3.cbor"));
/// let request: OSApplicationInfoReadRequest = cbor_static::from_slice(written)?;
/// assert_eq!(request.format, Some("snrvbmpioa"));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct OSApplicationInfoReadRequest<'a> {
    #[cbor(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub format: Option<&'a str>,
}

/// OS/application info's response: the requested fields, as one string.
///
/// ```
/// use cbor_static_conformance::smp::os::OSApplicationInfoReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/os_application_info_read_response/1.cbor"));
/// let response: OSApplicationInfoReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!(response.output, "the requested output string");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct OSApplicationInfoReadResponse<'a> {
    #[serde(borrow)]
    pub output: &'a str,
}

/// Bootloader info's response (command 8) without a query: the bootloader's name.
///
/// ```
/// use cbor_static_conformance::smp::os::BootloaderInformationReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/bootloader_information_read_response/1.cbor"));
/// let response: BootloaderInformationReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!(response.bootloader, "MCUboot");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct BootloaderInformationReadResponse<'a> {
    #[serde(borrow)]
    pub bootloader: &'a str,
}

/// Bootloader info (command 8) with a query, which `MCUboot` answers for `"mode"`. That the query
/// is `"mode"` is not checked.
///
/// ```
/// use cbor_static_conformance::smp::os::MCUbootModeReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/mc_uboot_mode_read_request/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<MCUbootModeReadRequest>(written)?.query, "mode");
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct MCUbootModeReadRequest<'a> {
    #[serde(borrow)]
    pub query: &'a str,
}

/// `MCUboot`'s mode, and whether it refuses downgrades. `no-downgrade` is sent only as `true`; that
/// is not checked.
///
/// ```
/// use cbor_static_conformance::smp::os::MCUbootModeReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/mc_uboot_mode_read_response/2.cbor"));
/// let response: MCUbootModeReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!((response.mode, response.no_downgrade), (9, Some(true)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct MCUbootModeReadResponse {
    pub mode: i64,
    #[serde(rename = "no-downgrade")]
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub no_downgrade: Option<bool>,
}

/// Task statistics (command 2): no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::TaskStatisticsReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/task_statistics_read_request/1.cbor"));
/// cbor_static::from_slice::<TaskStatisticsReadRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type TaskStatisticsReadRequest = super::Empty;

/// Memory pool statistics (command 3): no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::MemoryPoolStatisticsReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/memory_pool_statistics_read_request/1.cbor"));
/// cbor_static::from_slice::<MemoryPoolStatisticsReadRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type MemoryPoolStatisticsReadRequest = super::Empty;

/// Date-time read (command 4): no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::DateTimeReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/date_time_read_request/1.cbor"));
/// cbor_static::from_slice::<DateTimeReadRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type DateTimeReadRequest = super::Empty;

/// Date-time write (command 4): the time to set, shaped as the read's response.
///
/// ```
/// use cbor_static_conformance::smp::os::DateTimeWriteRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/date_time_write_request/1.cbor"));
/// cbor_static::from_slice::<DateTimeWriteRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type DateTimeWriteRequest<'a> = DateTimeReadResponse<'a>;

/// Date-time write's response: no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::DateTimeWriteResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/date_time_write_response/1.cbor"));
/// cbor_static::from_slice::<DateTimeWriteResponse>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type DateTimeWriteResponse = super::Empty;

/// Reset's response: no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::ResetWriteResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/reset_write_response/1.cbor"));
/// cbor_static::from_slice::<ResetWriteResponse>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type ResetWriteResponse = super::Empty;

/// `MCUmgr` parameters (command 6): no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::MCUMgrParametersReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/mcu_mgr_parameters_read_request/1.cbor"));
/// cbor_static::from_slice::<MCUMgrParametersReadRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type MCUMgrParametersReadRequest = super::Empty;

/// Bootloader info (command 8) without a query: no fields.
///
/// ```
/// use cbor_static_conformance::smp::os::BootloaderInformationReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/os/bootloader_information_read_request/1.cbor"));
/// cbor_static::from_slice::<BootloaderInformationReadRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type BootloaderInformationReadRequest = super::Empty;
