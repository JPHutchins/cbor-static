use cbor_static_conformance::smp::os::*;
use cbor_static_conformance::smp::smp_only;

use super::map;

include!(concat!(env!("SMP_VECTORS"), "/os_records.rs"));
