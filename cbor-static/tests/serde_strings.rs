#![cfg(feature = "serde")]
//! Strings in chunks through the serde layer (D-13a): a `char` and a bignum's digits read their
//! chunks into fixed state, as the native decoder does; with `alloc`, every other string is
//! assembled and handed on as owned, and without it is refused.

use cbor_static::ErrorKind;

/// `bytes` gives the same value, or the same kind at the same offset, through both layers.
fn alike<'a, T>(bytes: &'a [u8])
where
    T: serde::Deserialize<'a> + cbor_static::Decode<'a> + PartialEq + core::fmt::Debug,
{
    assert_eq!(
        cbor_static::serde::from_slice::<T>(bytes).map_err(|e| (e.kind, e.offset)),
        cbor_static::from_slice::<T>(bytes).map_err(|e| (e.kind, e.offset)),
        "{bytes:02x?}"
    );
}

#[test]
fn a_char_in_chunks_is_read_as_natively() {
    for bytes in [
        &b"\x7f\x61a\xff"[..],
        b"\x7f\x60\x61a\x60\xff",
        b"\x7f\x62\xc3\xa9\xff",
        b"\x7f\xff",
        b"\x7f\x61a\x61b\xff",
        b"\x7f\x61a\x63bcd\xff",
        b"\x7f\x61\xc3\x61\xa9\xff",
        b"\x7f\x41a\xff",
        b"\x7f\x61a",
        b"\x7f\x61",
    ] {
        alike::<char>(bytes);
    }
    assert_eq!(
        cbor_static::serde::from_slice::<char>(b"\x7f\x61a\xff"),
        Ok('a')
    );
}

#[test]
fn bignum_digits_in_chunks_are_read_as_natively() {
    let sixteen = b"\x50\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10";
    let seventeen = b"\x51\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01";
    for bytes in [
        b"\xc2\x5f\x41\x01\x41\x02\xff".to_vec(),
        b"\xc2\x5f\xff".to_vec(),
        [&b"\xc2\x5f\x41\x00"[..], sixteen, b"\xff"].concat(),
        [&b"\xc2\x5f"[..], seventeen, b"\x41\x01\xff"].concat(),
        b"\xc2\x5f\x48\x01\x02\x03\x04\x05\x06\x07\x08\x49\x01\x02\x03\x04\x05\x06\x07\x08\x09\xff"
            .to_vec(),
        b"\xc3\x5f\x41\x01\xff".to_vec(),
        b"\xc2\x5f\x61a\xff".to_vec(),
        b"\xc2\x5f\x41\x01".to_vec(),
    ] {
        alike::<u128>(&bytes);
        alike::<i128>(&bytes);
    }
    assert_eq!(
        cbor_static::serde::from_slice::<u128>(b"\xc2\x5f\x41\x01\x41\x02\xff"),
        Ok(0x102)
    );
}

#[cfg(feature = "alloc")]
#[test]
fn owned_strings_are_assembled_from_chunks() {
    assert_eq!(
        cbor_static::serde::from_slice::<String>(b"\x7f\x61a\x60\x62bc\xff").as_deref(),
        Ok("abc")
    );
    assert_eq!(
        cbor_static::serde::from_slice::<serde_bytes::ByteBuf>(b"\x5f\x41\x01\x42\x02\x03\xff")
            .as_deref()
            .map(Vec::as_slice),
        Ok(&[1u8, 2, 3][..])
    );
    assert_eq!(
        cbor_static::serde::from_slice::<String>(b"\x7f\xff").as_deref(),
        Ok("")
    );
}

#[cfg(feature = "alloc")]
#[test]
fn chunks_are_assembled_into_one_reservation() {
    let chunks = 1000;
    let input = [&b"\x7f"[..], &b"\x61a".repeat(chunks), b"\xff"].concat();
    let text = cbor_static::serde::from_slice::<String>(&input).expect("assembles");
    assert_eq!(text.len(), chunks);
    assert_eq!(text.capacity(), chunks);
}

#[cfg(feature = "alloc")]
#[test]
fn a_string_that_copies_through_deserialize_str_takes_chunks_too() {
    let text = b"\x7f\x62ab\x61c\xff";
    assert_eq!(
        cbor_static::serde::from_slice::<heapless::String<4>>(text).as_deref(),
        Ok("abc")
    );
    assert_eq!(
        cbor_static::serde::from_slice::<Box<str>>(text).as_deref(),
        Ok("abc")
    );
}

/// Whatever item comes, read through `deserialize_any`.
struct Anything;

impl<'de> serde::Deserialize<'de> for Anything {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(serde::de::IgnoredAny)
            .map(|_| Self)
    }
}

#[cfg(not(feature = "alloc"))]
#[test]
fn without_alloc_a_string_in_chunks_is_refused() {
    let refused = |bytes| {
        cbor_static::serde::from_slice::<Anything>(bytes)
            .map(|_| ())
            .map_err(|e| (e.kind, e.offset))
    };
    assert_eq!(
        refused(b"\x7f\x61a\xff"),
        Err((ErrorKind::IndefiniteString, 1))
    );
    assert_eq!(
        refused(b"\x5f\x41\x01\xff"),
        Err((ErrorKind::IndefiniteString, 1))
    );
    assert_eq!(
        cbor_static::serde::from_slice::<heapless::String<4>>(b"\x7f\x61a\xff")
            .map_err(|e| (e.kind, e.offset)),
        Err((ErrorKind::IndefiniteString, 1))
    );
}

#[cfg(feature = "alloc")]
#[test]
fn any_item_in_chunks_is_assembled() {
    assert!(
        cbor_static::serde::from_slice::<Anything>(b"\x82\x7f\x61a\xff\x5f\x41\x01\xff").is_ok()
    );
}

#[test]
fn a_struct_key_in_chunks_is_unsupported_as_natively() {
    #[derive(serde::Deserialize, cbor_static::Decode, Debug, PartialEq)]
    struct Point {
        x: u8,
    }

    alike::<Point>(b"\xa1\x7f\x61x\xff\x01");
    assert_eq!(
        cbor_static::serde::from_slice::<Point>(b"\xa1\x7f\x61x\xff\x01").map_err(|e| e.kind),
        Err(ErrorKind::Unsupported)
    );
}
