#![no_main]

use cbor_static_conformance::cose::{
    EncStructure, Encrypt, Encrypt0, HeaderMap, Headers, KdfContext, Key, KeySet, Mac, Mac0,
    MacStructure, Message, Protected, Recipient, SigStructure, SigStructure1, Sign, Sign1,
    Signature,
};
use cbor_static_conformance::cwt::{Claims, Tagged};

libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    cbor_static_fuzz::typed!(
        input,
        [
            Message<'_>,
            Sign<'_>,
            Signature<'_>,
            Sign1<'_>,
            Encrypt<'_>,
            Recipient<'_>,
            Encrypt0<'_>,
            Mac<'_>,
            Mac0<'_>,
            HeaderMap<'_>,
            Protected<'_>,
            Headers<'_>,
            Key<'_>,
            KeySet<'_>,
            SigStructure<'_>,
            SigStructure1<'_>,
            EncStructure<'_>,
            MacStructure<'_>,
            KdfContext<'_>,
            Claims<'_>,
            Tagged<'_>,
        ]
    );
});
