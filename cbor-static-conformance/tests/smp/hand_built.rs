//! The commands neither source covers, hand-built from Zephyr's `smp_group_1.rst`, and smp's own
//! encoding of an integer `force`: each decodes to the value it was built from, and re-encodes
//! byte for byte. Their bytes are smp's, so this is a check against smp, not against a device.

use cbor_static_conformance::smp::image::*;
use cbor_static_conformance::smp::smp_only;

use crate::records::{check, vec};

macro_rules! hand_built {
    ($($command:ident: $type:ty { ($twin:literal, $expected:expr) $(,)? })*) => {
        $(
            #[test]
            fn $command() {
                check::<$type>(twin!($twin), &$expected);
            }
        )*
    };
}

include!(concat!(env!("SMP_VECTORS"), "/hand_built.rs"));
