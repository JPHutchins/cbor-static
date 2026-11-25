#![no_main]

libfuzzer_sys::fuzz_target!(|input: &[u8]| fuzz_fixtures::each_rich!(
    cbor_static_fuzz::typed,
    input
));
