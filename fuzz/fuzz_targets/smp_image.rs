#![no_main]

use cbor_static_conformance::smp::image;

libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    cbor_static_fuzz::typed!(
        input,
        [
            image::ImageStatesReadResponse<'_>,
            image::ImageState<'_>,
            image::ImageStatesWriteRequest<'_>,
            image::ImageUploadWriteRequest<'_>,
            image::ImageUploadWriteResponse,
            image::ImageEraseRequest,
            image::ImageSlotInfoReadResponse,
            image::ImageSlotInfo,
            image::ImageSlot,
            image::ImageEraseResponse,
        ]
    );
});
