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
