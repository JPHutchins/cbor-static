#![no_main]

use std::collections::BTreeMap;

use fuzz_fixtures::forms::{
    Adjacent, External, Fields, Flattened, Internal, Newtype, Tuple, Unit, UnknownLength, Untagged,
};

libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    fuzz_fixtures::each_flat!(cbor_static_fuzz::layers, input);
    fuzz_fixtures::each_rich!(cbor_static_fuzz::layers, input);
    cbor_static_fuzz::deserializes!(
        input,
        [
            Unit,
            Newtype,
            Tuple,
            Fields,
            External,
            Internal,
            Adjacent,
            Untagged,
            Flattened,
            UnknownLength,
            Option<u32>,
            Option<()>,
            (),
            Vec<u32>,
            BTreeMap<String, u32>,
            (u8, bool, &str),
            Vec<u64>,
            Vec<i64>,
            Vec<f32>,
            Vec<f64>,
            &str,
            char,
            &serde_bytes::Bytes,
        ]
    );
});
