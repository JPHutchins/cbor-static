//! `Strict` over raw items and fixed-capacity maps (#12 Q29): a raw item's every head is
//! deterministic and its every map's keys rise bytewise, at most 8 containers deep, and a
//! `LinearMap`'s decoded keys rise bytewise by their encodings, chunk by chunk.

mod common;

use core::mem::MaybeUninit;

use cbor_static::{ErrorKind, MapRef, RawCbor};

fn strict<'de, T: cbor_static::Decode<'de>>(bytes: &'de [u8]) -> Result<(), ErrorKind> {
    let mut slot = MaybeUninit::<T>::uninit();
    cbor_static::decode_strict_into(bytes, &mut slot)
        .map(|_| ())
        .map_err(|e| e.kind)
}

/// `depth` arrays, each holding the next, around one integer.
fn nested(depth: usize) -> Vec<u8> {
    [vec![0x81; depth], vec![0x01]].concat()
}

#[test]
fn a_raw_item_is_deterministic_throughout() {
    for (bytes, expected) in [
        (b"\xa2\x01\x00\x61a\x00".as_slice(), Ok(())),
        (b"\xa2\x61a\x00\x01\x00", Err(ErrorKind::NotDeterministic)),
        (b"\xa2\x61b\x00\x62aa\x00", Ok(())),
        (b"\xa2\x62aa\x00\x61b\x00", Err(ErrorKind::NotDeterministic)),
        (b"\xa2\x01\x00\x01\x00", Err(ErrorKind::NotDeterministic)),
        (
            b"\x82\xa2\x02\x00\x01\x00\x00",
            Err(ErrorKind::NotDeterministic),
        ),
        (b"\xa1\x01\xa2\x01\x00\x02\x00", Ok(())),
        (
            b"\xa1\x01\xa2\x02\x00\x01\x00",
            Err(ErrorKind::NotDeterministic),
        ),
        (b"\xa2\xc1\x01\x00\xc1\x02\x00", Ok(())),
        (
            b"\xa2\xc1\x02\x00\xc1\x01\x00",
            Err(ErrorKind::NotDeterministic),
        ),
        (b"\x82\x18\x01\x00", Err(ErrorKind::NotDeterministic)),
        (b"\x82\x9f\xff\x00", Err(ErrorKind::NotDeterministic)),
        (
            b"\x81\xfa\x3f\x80\x00\x00",
            Err(ErrorKind::NotDeterministic),
        ),
    ] {
        assert_eq!(strict::<RawCbor>(bytes), expected, "{bytes:x?}");
        assert_eq!(
            common::decode::<RawCbor>(bytes).map(|_| ()),
            Ok(()),
            "not strict: {bytes:x?}"
        );
    }
}

#[test]
fn a_raw_item_nests_at_most_8_containers_under_strict() {
    assert_eq!(strict::<RawCbor>(&nested(8)), Ok(()));
    assert_eq!(strict::<RawCbor>(&nested(9)), Err(ErrorKind::LimitExceeded));
    assert_eq!(common::decode::<RawCbor>(&nested(9)).map(|_| ()), Ok(()));
}

#[test]
fn a_map_ref_needs_rising_keys_under_strict() {
    assert_eq!(
        strict::<MapRef<&str, u8>>(b"\xa2\x61a\x01\x61b\x02"),
        Ok(())
    );
    assert_eq!(
        strict::<MapRef<&str, u8>>(b"\xa2\x61b\x01\x61a\x02"),
        Err(ErrorKind::NotDeterministic)
    );
}

#[cfg(feature = "heapless")]
mod linear_map {
    use cbor_static::{Decoder, ErrorKind};
    use heapless::{LinearMap, String};

    use super::strict;

    /// Whether `bytes` decode strictly as `T` in input chunks of every size.
    fn strict_in_any_chunks<T>(bytes: &[u8]) -> Result<(), ErrorKind>
    where
        T: for<'de> cbor_static::Decode<'de>,
    {
        (1..=bytes.len()).try_for_each(|chunk| {
            let mut slot = core::mem::MaybeUninit::<T>::uninit();
            let mut decoder = Decoder::new_strict(&mut slot);
            bytes
                .chunks(chunk)
                .try_for_each(|piece| decoder.feed(piece).map(|_| ()))
                .and_then(|()| decoder.finish().map(|_| ()))
                .map_err(|e| e.kind)
        })
    }

    #[test]
    fn its_keys_rise_by_their_encoding_in_any_chunks() {
        type Ints = LinearMap<i32, bool, 4>;
        type Texts = LinearMap<String<4>, u8, 4>;
        for (bytes, expected) in [
            (b"\xa3\x01\xf5\x18\x18\xf4\x20\xf5".as_slice(), Ok(())),
            (b"\xa2\x20\xf5\x01\xf4", Err(ErrorKind::NotDeterministic)),
            (b"\xa2\x01\xf5\x01\xf4", Err(ErrorKind::NotDeterministic)),
        ] {
            assert_eq!(strict_in_any_chunks::<Ints>(bytes), expected, "{bytes:x?}");
        }
        assert_eq!(
            strict_in_any_chunks::<Texts>(b"\xa2\x61b\x01\x62aa\x02"),
            Ok(())
        );
        assert_eq!(
            strict_in_any_chunks::<Texts>(b"\xa2\x62aa\x02\x61b\x01"),
            Err(ErrorKind::NotDeterministic)
        );
    }

    #[test]
    fn a_key_with_nesting_is_compared_on_the_map_own_spare_frames() {
        type Pairs = LinearMap<(u8, u8), u8, 4>;
        assert_eq!(
            strict::<Pairs>(b"\xa2\x82\x01\x02\x00\x82\x01\x03\x00"),
            Ok(())
        );
        assert_eq!(
            strict::<Pairs>(b"\xa2\x82\x01\x03\x00\x82\x01\x02\x00"),
            Err(ErrorKind::NotDeterministic)
        );
        let mut slot = core::mem::MaybeUninit::<Pairs>::uninit();
        assert_eq!(
            cbor_static::decode_into(b"\xa2\x82\x01\x03\x00\x82\x01\x02\x00", &mut slot)
                .map(|map| map.len())
                .map_err(|e| e.kind),
            Ok(2)
        );
    }
}
