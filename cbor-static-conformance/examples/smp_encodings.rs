//! Prints, one JSON line each, the SMP payloads `harness.smp_oracle` feeds JPHutchins/smp's
//! deserializer: cbor-static's encoding of each twin `oracle.json` lists, read at run time and
//! decoded as its Rust type; each firmware frame as the firmware test holds it; and cbor-static's
//! encoding of the firmware's shapes no fixture holds, built here.

use cbor_static::Encode;
use cbor_static_conformance::smp::{ErrorV1, ErrorV2, image, os, smp_only};
use serde::{Deserialize, Serialize};

const VECTORS: &str = env!("SMP_VECTORS");
const HEADER: usize = 8;

/// The cases `nix/smp_vectors/` lists for the oracle.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    rust: String,
    smp: String,
    entries: Option<String>,
    firmware: bool,
}

/// A payload for smp to read as its class, under `header` when it has one, and whether cbor-static
/// decodes it; `diverges` pins a shape one of them is known to refuse.
#[derive(Serialize)]
struct Line<'a> {
    name: String,
    smp: &'a str,
    entries: Option<&'a str>,
    diverges: Option<Pin>,
    header: Option<String>,
    payload: String,
    rejected_by_cbor_static: Option<String>,
}

/// The issue that records a reader refusing a shape, and what its refusal says.
#[derive(Clone, Copy, Serialize)]
struct Pin {
    issue: &'static str,
    reason: &'static str,
}

macro_rules! oracle_types {
    ($(($name:literal, $type:ty)),* $(,)?) => {
        /// cbor-static's encoding of `payload` decoded as the Rust type named `rust`, or the kind of
        /// error it fails to decode with.
        fn reencoded(rust: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
            match rust {
                $($name => decoded_and_encoded::<$type>(payload),)*
                unknown => panic!("oracle.json names {unknown}, which oracle_types.rs does not list"),
            }
        }
    };
}

include!(concat!(env!("SMP_VECTORS"), "/oracle_types.rs"));

fn main() {
    serde_json::from_slice::<Manifest>(&read("oracle.json"))
        .expect("oracle.json is the manifest nix/smp_vectors/ writes")
        .cases
        .iter()
        .flat_map(lines)
        .chain(built())
        .for_each(|line| {
            println!(
                "{json}",
                json = serde_json::to_string(&line).expect("a line serializes")
            );
        });
}

/// The case's twin as cbor-static writes it; for a firmware case, also the firmware's frame, and
/// cbor-static's encoding under the firmware's header.
fn lines(case: &Case) -> Vec<Line<'_>> {
    let twin = read(&format!("{name}.cbor", name = case.name));
    let encoding = reencoded(&case.rust, &twin);
    let firmware = case
        .firmware
        .then(|| read(&format!("{name}.frame", name = case.name)));
    let line = |suffix: &str, header: Option<&[u8]>, payload: &[u8]| Line {
        name: format!("{name}{suffix}", name = case.name),
        smp: &case.smp,
        entries: case.entries.as_deref(),
        diverges: None,
        header: header.map(hex),
        payload: hex(payload),
        rejected_by_cbor_static: encoding.as_ref().err().cloned(),
    };
    firmware
        .iter()
        .map(|frame| line(".frame", Some(&frame[..HEADER]), &frame[HEADER..]))
        .chain([line(
            "",
            firmware.as_deref().map(|frame| &frame[..HEADER]),
            encoding.as_deref().unwrap_or(&twin),
        )])
        .collect()
}

/// Zephyr's shapes that neither smp's records nor Zephyr's tests hold, as cbor-static writes them,
/// each pinned to the issue that records a reader refusing it, with how cbor-static's firmware type
/// reads it. The firmware's pools with a `blksiz` are also put at smp's root, so `blksiz` is the
/// only difference; the repeated task names are spliced from cbor-static's encodings, as no
/// cbor-static type writes them. Their bytes are cbor-static's, not a device's.
fn built() -> [Line<'static>; 6] {
    use cbor_static_conformance::smp::image::{ImageEraseResponse, ImageSlotInfoReadResponse};
    use cbor_static_conformance::smp::os::{
        MEMORY_POOLS, MemoryPoolStatistics, MemoryPoolStatisticsReadResponse, ResetWriteRequest,
        TaskStatisticsReadResponse,
    };
    use heapless::LinearMap;

    const MPSTAT: &str = "smp.os_management.MemoryPoolStatisticsReadResponse";
    const POOL: Option<&str> = Some("smp.os_management.MemoryPoolStatistics");
    [
        built_line(
            "built/os/memory_pool_statistics_read_response/mpools",
            MPSTAT,
            POOL,
            Pin {
                issue: "JPHutchins/smp#77",
                reason: "smp keeps ['mpools'] unread",
            },
            &encoded(&pools(None)),
            |payload| refused::<MemoryPoolStatisticsReadResponse>(payload),
        ),
        built_line(
            "built/os/memory_pool_statistics_read_response/blksiz_at_the_root",
            MPSTAT,
            POOL,
            Pin {
                issue: "JPHutchins/smp#78",
                reason: "smp keeps ['0'] unread",
            },
            &encoded(&pools(Some(1)).mpools),
            |payload| refused::<LinearMap<&str, MemoryPoolStatistics, MEMORY_POOLS>>(payload),
        ),
        built_line(
            "built/os/reset_write_request/force_bool",
            "smp.os_management.ResetWriteRequest",
            None,
            Pin {
                issue: "JPHutchins/smp#79",
                reason: "smp writes a165666f72636501",
            },
            &encoded(&ResetWriteRequest {
                force: Some(true),
                boot_mode: None,
            }),
            |payload| refused::<ResetWriteRequest>(payload),
        ),
        built_line(
            "built/image/image_slot_info_read_response/slot_rc",
            "smp.image_management.ImageSlotInfoReadResponse",
            None,
            Pin {
                issue: "JPHutchins/smp#80",
                reason: "images.0.slots.1.rc",
            },
            &encoded(&a_slot_that_failed_to_open()),
            |payload| refused::<ImageSlotInfoReadResponse>(payload),
        ),
        built_line(
            "built/image/image_erase_response/legacy_rc",
            "smp.image_management.ImageEraseResponse",
            None,
            Pin {
                issue: "JPHutchins/smp#81",
                reason: "ImageEraseResponse\nrc\n  Extra inputs are not permitted",
            },
            &encoded(&ImageEraseResponse { rc: Some(0) }),
            |payload| refused::<ImageEraseResponse>(payload),
        ),
        built_line(
            "built/os/task_statistics_read_response/repeated_names",
            "smp.os_management.TaskStatisticsReadResponse",
            None,
            Pin {
                issue: "zephyrproject-rtos/zephyr#126661",
                reason: "DuplicateKey",
            },
            &repeated_task_names(),
            |payload| refused::<TaskStatisticsReadResponse>(payload),
        ),
    ]
}

/// One pool under `"mpools"` (`os_mgmt.c:393`), with `blksiz` only when given
/// (`os_mgmt.c:421-424`).
fn pools(blksiz: Option<i64>) -> os::MemoryPoolStatisticsReadResponse<'static> {
    os::MemoryPoolStatisticsReadResponse {
        mpools: [(
            "0",
            os::MemoryPoolStatistics {
                blksiz,
                nblks: 2048,
                nfree: 1024,
                min: 512,
            },
        )]
        .into_iter()
        .collect(),
    }
}

/// Slot info whose second slot failed to open: `rc` in place of `size` (`img_mgmt.c:558-565`).
fn a_slot_that_failed_to_open() -> image::ImageSlotInfoReadResponse {
    use cbor_static_conformance::smp::image::{
        ImageSlot, ImageSlotInfo, ImageSlotInfoReadResponse,
    };

    ImageSlotInfoReadResponse {
        images: [ImageSlotInfo {
            image: 0,
            slots: [
                ImageSlot {
                    slot: 0,
                    size: Some(0x6_0000),
                    rc: None,
                    upload_image_id: None,
                },
                ImageSlot {
                    slot: 1,
                    size: None,
                    rc: Some(-2),
                    upload_image_id: None,
                },
            ]
            .into_iter()
            .collect(),
            max_image_size: None,
        }]
        .into_iter()
        .collect(),
    }
}

/// Two tasks at priority -1, each named by its priority as a default Zephyr build names it
/// (`os_mgmt.c:221-233`, zephyrproject-rtos/zephyr#126661).
fn repeated_task_names() -> Vec<u8> {
    let task = |tid| {
        encoded(&os::TaskStatisticsZephyr {
            prio: -1,
            tid,
            state: 1,
            ..os::TaskStatisticsZephyr::default()
        })
    };
    [
        b"\xa1".to_vec(),
        encoded(&"tasks"),
        b"\xa2".to_vec(),
        encoded(&"-1"),
        task(1),
        encoded(&"-1"),
        task(2),
    ]
    .concat()
}

fn built_line(
    name: &str,
    smp: &'static str,
    entries: Option<&'static str>,
    pin: Pin,
    payload: &[u8],
    refused_by_cbor_static: fn(&[u8]) -> Option<String>,
) -> Line<'static> {
    Line {
        name: name.to_owned(),
        smp,
        entries,
        diverges: Some(pin),
        header: None,
        rejected_by_cbor_static: refused_by_cbor_static(payload),
        payload: hex(payload),
    }
}

fn refused<'a, T: cbor_static::Decode<'a>>(payload: &'a [u8]) -> Option<String> {
    cbor_static::from_slice::<T>(payload)
        .err()
        .map(|error| format!("{kind:?}", kind = error.kind))
}

fn decoded_and_encoded<'a, T: cbor_static::Decode<'a> + Encode>(
    payload: &'a [u8],
) -> Result<Vec<u8>, String> {
    cbor_static::from_slice::<T>(payload)
        .map(|value| encoded(&value))
        .map_err(|error| format!("{kind:?}", kind = error.kind))
}

fn encoded<T: Encode>(value: &T) -> Vec<u8> {
    cbor_static::to_vec(value).expect("a fixture's value encodes")
}

fn read(file: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(VECTORS).join(file))
        .expect("SMP_VECTORS holds the manifest and each twin it names")
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 0xf)],
            ]
        })
        .map(char::from)
        .collect()
}
