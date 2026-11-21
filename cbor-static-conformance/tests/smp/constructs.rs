//! Every shape the OS and Image groups' payloads take, one test each: how the SMP types express
//! it, or, where the derive cannot yet, the nearest form and the check it needs after decoding.

use cbor_static::ErrorKind;

use crate::common::encode;

/// JPHutchins/smp writes maps in RFC 7049's canonical order, shorter keys first. The fields are
/// declared in `smp_group_1.rst`'s order, `image` first, and `#[cbor(length_first)]` sorts them.
#[test]
fn keys_are_written_shorter_first() {
    use cbor_static_conformance::smp::image::ImageUploadWriteRequest;

    let written = twin!("image/image_upload_write_request/1");
    let request: ImageUploadWriteRequest = cbor_static::from_slice(written).expect("a record");
    assert_eq!(
        (&written[..5], request.image),
        (b"\xa6\x63len".as_slice(), Some(1))
    );
    assert_eq!(encode(&request).as_deref(), Ok(written));
}

/// `{ * tstr => task }`: tasks keyed by name are a `LinearMap`, which a length-first type writes
/// sorted, whatever order its entries were inserted in.
#[test]
fn a_map_keyed_by_name_is_a_linear_map() {
    use cbor_static_conformance::smp::os::TaskStatisticsReadResponse;

    let written = twin!("os/task_statistics_read_response/1");
    let response: TaskStatisticsReadResponse = cbor_static::from_slice(written).expect("a record");
    let reversed = TaskStatisticsReadResponse {
        tasks: response
            .tasks
            .iter()
            .map(|(name, task)| (*name, *task))
            .collect::<std::vec::Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
    };
    assert_ne!(reversed.tasks.keys().next(), response.tasks.keys().next());
    assert_eq!(encode(&reversed).as_deref(), Ok(written));
}

/// A default Zephyr build names each task by its priority, so two tasks at one priority repeat a
/// key (zephyrproject-rtos/zephyr#126661). A map whose keys repeat is not valid CBOR (RFC 8949
/// §5.6), so it decodes as `DuplicateKey`, where smp keeps the last (#64).
#[test]
fn a_repeated_task_name_is_a_duplicate_key() {
    use cbor_static_conformance::smp::os::{TaskStatisticsReadResponse, TaskStatisticsZephyr};

    let (name, task) = (
        encode(&"-1").expect("the name encodes"),
        encode(&TaskStatisticsZephyr {
            prio: -1,
            tid: 1,
            state: 1,
            ..TaskStatisticsZephyr::default()
        })
        .expect("the task encodes"),
    );
    assert_eq!(
        cbor_static::from_slice::<TaskStatisticsReadResponse>(
            &[b"\xa1\x65tasks\xa2".as_slice(), &name, &task, &name, &task].concat()
        )
        .map_err(|e| e.kind),
        Err(ErrorKind::DuplicateKey)
    );
}

/// Zephyr lists every thread and every heap, so a response can hold more tasks or pools than the
/// types' capacities; one more decodes as `OutOfRange`.
#[test]
fn one_more_task_or_pool_than_the_capacity_is_out_of_range() {
    use cbor_static_conformance::smp::os::{
        MEMORY_POOLS, MemoryPoolStatistics, MemoryPoolStatisticsReadResponse, TASKS,
        TaskStatisticsReadResponse, TaskStatisticsZephyr,
    };
    use heapless::LinearMap;

    let names: std::vec::Vec<std::string::String> = (0..=TASKS).map(|n| n.to_string()).collect();
    assert_eq!(
        cbor_static::from_slice::<TaskStatisticsReadResponse>(
            &[
                b"\xa1\x65tasks".as_slice(),
                &encode(
                    &names
                        .iter()
                        .map(|name| (name.as_str(), TaskStatisticsZephyr::default()))
                        .collect::<LinearMap<_, _, { TASKS + 1 }>>()
                )
                .expect("the tasks encode"),
            ]
            .concat()
        )
        .map_err(|e| e.kind),
        Err(ErrorKind::OutOfRange)
    );
    assert_eq!(
        cbor_static::from_slice::<MemoryPoolStatisticsReadResponse>(
            &[
                b"\xa1\x66mpools".as_slice(),
                &encode(
                    &names[..=MEMORY_POOLS]
                        .iter()
                        .map(|name| {
                            (
                                name.as_str(),
                                MemoryPoolStatistics {
                                    blksiz: None,
                                    nblks: 1,
                                    nfree: 1,
                                    min: 1,
                                },
                            )
                        })
                        .collect::<LinearMap<_, _, { MEMORY_POOLS + 1 }>>()
                )
                .expect("the pools encode"),
            ]
            .concat()
        )
        .map_err(|e| e.kind),
        Err(ErrorKind::OutOfRange)
    );
}

/// `? key => value`: an `Option` that is skipped when `None`, so an absent key stays absent.
#[test]
fn an_optional_key_is_omitted_when_none() {
    use cbor_static_conformance::smp::image::ImageUploadWriteResponse;

    assert_eq!(
        encode(&ImageUploadWriteResponse::default()).as_deref(),
        Ok(twin!("image/image_upload_write_response/1"))
    );
}

/// Zephyr writes indefinite-length maps (`bf … ff`), and cbor-static writes definite lengths
/// only, so the firmware's bytes cannot round-trip. The nearest form is comparing them as data
/// (#64).
#[test]
fn an_indefinite_length_map_is_compared_as_data() {
    use cbor_static_conformance::smp::os::EchoWriteResponse;

    use crate::common::reencode;

    let firmware = twin!("zephyr/os_mgmt_echo/expected_response");
    let response: EchoWriteResponse = cbor_static::from_slice(firmware).expect("firmware's map");
    let definite = encode(&response).expect("the response encodes");
    assert_ne!(definite, firmware);
    assert_eq!(
        reencode::<EchoWriteResponse>(&definite).as_deref(),
        Ok(definite.as_slice())
    );
    assert_eq!(
        cbor_static::from_slice::<EchoWriteResponse>(&definite),
        Ok(response)
    );
}

/// A response is the command's map or an error map. Both begin with a map's head, so an untagged
/// enum cannot tell them apart by the first byte; the nearest form is an entry point for each,
/// chosen by context (#64).
#[test]
fn a_response_or_an_error_is_an_entry_point_each() {
    use cbor_static_conformance::smp::ErrorV1;
    use cbor_static_conformance::smp::os::EchoWriteResponse;

    let (success, error) = (
        twin!("zephyr/os_mgmt_echo/expected_response"),
        twin!("zephyr/os_mgmt_echo/expected_response_no_data"),
    );
    assert!(cbor_static::from_slice::<EchoWriteResponse>(success).is_ok());
    assert!(cbor_static::from_slice::<ErrorV1>(error).is_ok());
    assert_eq!(
        cbor_static::from_slice::<ErrorV1>(success).map_err(|e| e.kind),
        Err(ErrorKind::UnknownField)
    );
    assert_eq!(
        cbor_static::from_slice::<EchoWriteResponse>(error).map_err(|e| e.kind),
        Err(ErrorKind::UnknownField)
    );
}

/// smp reads a task as `TaskStatistics`, every field present, or as `TaskStatisticsZephyr`, the
/// fields Zephyr's Kconfig selects. A choice between maps has no expression yet, so the nearest
/// form is the superset, which reads both; `TaskStatistics` rejects the firmware's subset (#64).
#[test]
fn a_choice_between_maps_is_their_superset() {
    use cbor_static_conformance::smp::os::{TaskStatistics, TaskStatisticsZephyr};

    let subset = encode(&TaskStatisticsZephyr {
        prio: -1,
        tid: 0,
        state: 1,
        ..TaskStatisticsZephyr::default()
    })
    .expect("the subset encodes");
    assert!(cbor_static::from_slice::<TaskStatisticsZephyr>(&subset).is_ok());
    assert!(
        cbor_static::from_slice::<TaskStatisticsZephyr>(
            &encode(&TaskStatistics::default()).expect("every field encodes")
        )
        .is_ok()
    );
    assert_eq!(
        cbor_static::from_slice::<TaskStatistics>(&subset).map_err(|e| e.kind),
        Err(ErrorKind::MissingField)
    );
}

/// `"query": "mode"` and `"no-downgrade": true` are one-value ranges, and Zephyr refuses a
/// `boot_mode` above 255 after decoding it as a `u32` (`os_mgmt.c:483, 493-495`). Today's
/// vocabulary has no literal-valued or ranged field, so they are a `&str`, a `bool` and a `u32`,
/// and any other value decodes; the range is a check after decoding (#64).
#[test]
fn a_literal_value_or_a_range_is_not_checked() {
    use cbor_static_conformance::smp::os::{
        MCUbootModeReadRequest, MCUbootModeReadResponse, ResetWriteRequest,
    };

    assert_eq!(
        cbor_static::from_slice::<MCUbootModeReadRequest>(
            &encode(&MCUbootModeReadRequest { query: "other" }).expect("the request encodes")
        )
        .map(|request| request.query),
        Ok("other")
    );
    assert_eq!(
        cbor_static::from_slice::<MCUbootModeReadResponse>(
            &encode(&MCUbootModeReadResponse {
                mode: 0,
                no_downgrade: Some(false),
            })
            .expect("the response encodes")
        )
        .map(|response| response.no_downgrade),
        Ok(Some(false))
    );
    assert_eq!(
        cbor_static::from_slice::<ResetWriteRequest>(b"\xa1\x69boot_mode\x19\x01\x2c")
            .map(|request| request.boot_mode),
        Ok(Some(300))
    );
}
