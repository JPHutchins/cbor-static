#![no_main]

use cbor_static_conformance::edhoc::{
    ConnectionId, Context2, Context3, Cred, Ead, Eads, ErrorMessage, IdCred, Info, Message1,
    Message2, Message3, Message4, Plaintext2, Plaintext3, Plaintext4, Suites, Th2Input,
    TranscriptInput,
};

libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    cbor_static_fuzz::typed!(
        input,
        [
            Message2<'_>,
            Message3<'_>,
            Message4<'_>,
            Suites,
            ConnectionId<'_>,
            IdCred<'_>,
            Cred<'_>,
        ]
    );
    cbor_static_fuzz::sequences!(
        input,
        [
            Message1<'_>,
            Plaintext2<'_>,
            Plaintext3<'_>,
            Plaintext4<'_>,
            ErrorMessage<'_>,
            Ead<'_>,
            Eads<'_>,
            Info<'_>,
            Context2<'_>,
            Context3<'_>,
            Th2Input<'_>,
            TranscriptInput<'_, Plaintext3<'_>>,
            TranscriptInput<'_, Plaintext4<'_>>,
        ]
    );
});
