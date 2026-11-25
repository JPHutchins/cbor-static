#![no_main]

libfuzzer_sys::fuzz_target!(|input: &[u8]| cbor_static_fuzz::tokens::agree(input));
