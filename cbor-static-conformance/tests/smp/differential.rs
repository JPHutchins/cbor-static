//! What the SMP types write, through ciborium, minicbor and cbor2: one case per twin the smp oracle
//! reads, which is the first record of each kwargs shape, every Zephyr firmware twin and every
//! hand-built one, as its type writes what it decodes.

use cbor_static_conformance::smp::{ErrorV1, ErrorV2, image, os, smp_only};

use crate::common::differential::typed::round_trip;
use crate::common::reencode;

/// The twin its type refuses, so it writes nothing: Zephyr's echo command without the `d`
/// `os::EchoWriteRequest` requires.
const REFUSED: [&str; 1] = ["zephyr/os_mgmt_echo/command_no_data"];

macro_rules! oracle_cases {
    ($(($test:ident, $name:literal, $type:ty)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                match reencode::<$type>(twin!($name)) {
                    Ok(ours) => {
                        assert!(!REFUSED.contains(&$name));
                        round_trip(&ours, |bytes| reencode::<$type>(bytes));
                    }
                    Err(error) => assert!(REFUSED.contains(&$name), "{error:?}"),
                }
            }
        )*
    };
}

include!(concat!(env!("SMP_VECTORS"), "/oracle_cases.rs"));
