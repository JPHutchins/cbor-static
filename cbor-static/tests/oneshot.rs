//! Decoding from whole input, which every engine build passes, the one-shot one included: the
//! `oneshot` task runs this file under `--cfg cbor_static_oneshot`, whose drain moves an item all at
//! once or not at all.
#![cfg(feature = "heapless")]

use core::mem::MaybeUninit;

use cbor_static::ErrorKind;
use heapless::{LinearMap, String};

/// Strict compares `LinearMap` keys by their encodings a byte at a time, even when a key's head and
/// payload are pending together.
#[test]
fn strict_compares_multi_byte_keys_of_a_linear_map() {
    type Texts = LinearMap<String<4>, u8, 4>;
    for (bytes, expected) in [
        (b"\xa2\x61b\x01\x62aa\x02".as_slice(), Ok(())),
        (b"\xa2\x62aa\x02\x61b\x01", Err(ErrorKind::NotDeterministic)),
    ] {
        let mut slot = MaybeUninit::<Texts>::uninit();
        assert_eq!(
            cbor_static::decode_strict_into(bytes, &mut slot)
                .map(|_| ())
                .map_err(|e| e.kind),
            expected,
            "{bytes:x?}"
        );
    }
}

/// The one-shot drain cannot split an item across scratch buffers, so a buffer shorter than an
/// item is too small, where the resumable engine writes the item in parts.
#[cfg(feature = "embedded-io")]
#[test]
fn a_scratch_buffer_shorter_than_an_item_is_too_small_only_in_one_shot() {
    let mut out = [0u8; 8];
    let written =
        cbor_static::embedded_io::to_writer(&70_000u32, &mut out.as_mut_slice(), &mut [0u8; 4])
            .map_err(|error| match error {
                cbor_static::IoError::Cbor(error) => Some((error.kind, error.offset)),
                cbor_static::IoError::Io(_) => None,
            });
    assert_eq!(
        written,
        if cfg!(cbor_static_oneshot) {
            Err(Some((ErrorKind::BufferTooSmall, 0)))
        } else {
            Ok(5)
        }
    );
}
