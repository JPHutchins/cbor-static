//! #5 Q15: `#[cbor(max_len = N)]` bounds a field's `len()`. Encoding a longer one fails with
//! `LimitExceeded`; decoding rejects a longer definite length at its head, before any copy, and an
//! indefinite one as its chunks or elements arrive.

mod common;

use cbor_static::{Encode, ErrorKind, to_slice};
use common::{chunked_decode, decode, encode};

#[derive(cbor_static::Cbor, Clone, Copy, Debug, PartialEq)]
struct Borrowed<'a> {
    #[cbor(max_len = 4)]
    name: &'a str,
    #[cbor(max_len = 2)]
    raw: &'a [u8],
    #[cbor(max_len = 3)]
    #[serde(skip_serializing_if = "Option::is_none")]
    nick: Option<&'a str>,
}

fn encode_kind<T: Encode>(value: &T) -> Result<usize, ErrorKind> {
    to_slice(value, &mut [0u8; 64]).map_err(|e| e.kind)
}

#[test]
fn borrowed_strings_at_their_limit_round_trip() {
    let value = Borrowed {
        name: "abcd",
        raw: &[1, 2],
        nick: Some("xyz"),
    };
    let bytes = encode(&value);
    assert_eq!(
        bytes,
        b"\xa3\x64name\x64abcd\x63raw\x42\x01\x02\x64nick\x63xyz"
    );
    assert_eq!(decode::<Borrowed>(&bytes), Ok(value));
    assert_eq!(
        decode::<Borrowed>(b"\xa2\x64name\x60\x63raw\x40"),
        Ok(Borrowed {
            name: "",
            raw: &[],
            nick: None
        })
    );
}

#[test]
fn encoding_past_the_limit_fails() {
    let at = Borrowed {
        name: "abcd",
        raw: &[],
        nick: None,
    };
    assert_eq!(
        encode_kind(&Borrowed {
            name: "abcde",
            ..at
        }),
        Err(ErrorKind::LimitExceeded)
    );
    assert_eq!(
        encode_kind(&Borrowed { raw: &[0; 3], ..at }),
        Err(ErrorKind::LimitExceeded)
    );
    assert_eq!(
        encode_kind(&Borrowed {
            nick: Some("wxyz"),
            ..at
        }),
        Err(ErrorKind::LimitExceeded)
    );
    assert_eq!(
        cbor_static::encoded_len(&Borrowed {
            name: "abcde",
            ..at
        })
        .map_err(|e| e.kind),
        Err(ErrorKind::LimitExceeded)
    );
}

#[test]
fn decoding_past_the_limit_fails_whole_and_in_chunks() {
    for bytes in [
        &b"\xa2\x64name\x65abcde\x63raw\x40"[..],
        b"\xa2\x64name\x60\x63raw\x43\x01\x02\x03",
        b"\xa3\x64name\x60\x63raw\x40\x64nick\x64wxyz",
    ] {
        assert_eq!(decode::<Borrowed>(bytes), Err(ErrorKind::LimitExceeded));
        assert_eq!(
            chunked_decode::<Borrowed>(bytes, bytes.len()),
            Err(ErrorKind::LimitExceeded)
        );
    }
}

#[cfg(feature = "heapless")]
mod bounded {
    use cbor_static::ErrorKind;
    use heapless::{LinearMap, String, Vec};

    use core::mem::MaybeUninit;

    use cbor_static::{Decode, Decoder, Fed};

    use super::encode_kind;
    use crate::common::golden_in_place;

    /// Feeds `bytes` in `chunk`-byte pieces, returning how many pieces went in before the decoder
    /// failed, with the error, or `None` once it finished.
    fn fails_after<'de, T: Decode<'de>>(
        bytes: &'de [u8],
        chunk: usize,
    ) -> Option<(usize, ErrorKind)> {
        let mut slot = MaybeUninit::<T>::uninit();
        let mut decoder = Decoder::new(&mut slot);
        bytes
            .chunks(chunk)
            .enumerate()
            .find_map(|(fed, piece)| match decoder.feed(piece) {
                Ok(Fed::NeedInput | Fed::Done(_)) => None,
                Err(error) => Some((fed + 1, error.kind)),
            })
    }

    #[derive(cbor_static::Cbor, Debug, PartialEq)]
    struct Owned {
        #[cbor(max_len = 4)]
        name: String<16>,
        #[cbor(max_len = 2)]
        values: Vec<u16, 8>,
        #[cbor(bytes, max_len = 3)]
        raw: Vec<u8, 8>,
        #[cbor(max_len = 1)]
        map: LinearMap<u8, bool, 4>,
    }

    fn at_limit() -> Owned {
        Owned {
            name: String::try_from("abcd").expect("fits"),
            values: Vec::from_slice(&[1, 2]).expect("fits"),
            raw: Vec::from_slice(&[7, 8, 9]).expect("fits"),
            map: [(1, true)].into_iter().collect(),
        }
    }

    const AT_LIMIT: &[u8] =
        b"\xa4\x64name\x64abcd\x66values\x82\x01\x02\x63raw\x43\x07\x08\x09\x63map\xa1\x01\xf5";

    #[test]
    fn owned_containers_at_their_limit_round_trip_in_chunks() {
        golden_in_place(&at_limit(), AT_LIMIT);
    }

    #[test]
    fn encoding_past_the_limit_fails() {
        let longer = |change: fn(&mut Owned)| {
            let mut value = at_limit();
            change(&mut value);
            encode_kind(&value)
        };
        assert_eq!(
            longer(|v| v.name.push('e').expect("fits")),
            Err(ErrorKind::LimitExceeded)
        );
        assert_eq!(
            longer(|v| v.values.push(3).expect("fits")),
            Err(ErrorKind::LimitExceeded)
        );
        assert_eq!(
            longer(|v| v.raw.push(0).expect("fits")),
            Err(ErrorKind::LimitExceeded)
        );
        assert_eq!(
            longer(|v| assert_eq!(v.map.insert(2, false), Ok(None))),
            Err(ErrorKind::LimitExceeded)
        );
    }

    /// The field's head and one byte of a string or element longer than the limit: the head
    /// alone rejects it.
    #[test]
    fn a_longer_definite_length_is_rejected_at_its_head() {
        for head in [
            &b"\xa1\x64name\x65a"[..],
            b"\xa1\x66values\x83\x01",
            b"\xa1\x63raw\x44\x01",
            b"\xa1\x63map\xa2\x01",
        ] {
            assert_eq!(
                fails_after::<Owned>(head, head.len() - 1),
                Some((1, ErrorKind::LimitExceeded)),
                "{head:x?}"
            );
        }
    }

    /// An indefinite string fails at the chunk that would pass the limit, and an indefinite array
    /// or map at the element past it.
    #[test]
    fn a_longer_indefinite_length_fails_as_it_arrives() {
        for (bytes, error) in [
            (
                &b"\xa1\x64name\x7f\x62ab\x63cde\xff"[..],
                ErrorKind::LimitExceeded,
            ),
            (
                b"\xa1\x63raw\x5f\x42\x01\x02\x42\x03\x04\xff",
                ErrorKind::LimitExceeded,
            ),
            (b"\xa1\x66values\x9f\x01\x02\x03\xff", ErrorKind::OutOfRange),
            (
                b"\xa1\x63map\xbf\x01\xf5\x02\xf4\xff",
                ErrorKind::OutOfRange,
            ),
        ] {
            for chunk in 1..=bytes.len() {
                assert_eq!(
                    fails_after::<Owned>(bytes, chunk).map(|(_, kind)| kind),
                    Some(error),
                    "{bytes:x?} in chunks of {chunk}"
                );
            }
        }
    }

    #[derive(cbor_static::Cbor, Debug, PartialEq)]
    struct Optional {
        #[cbor(max_len = 2)]
        tag: Option<String<8>>,
    }

    #[test]
    fn an_option_is_bounded_when_some() {
        golden_in_place(&Optional { tag: None }, b"\xa1\x63tag\xf6");
        golden_in_place(
            &Optional {
                tag: Some(String::try_from("ab").expect("fits")),
            },
            b"\xa1\x63tag\x62ab",
        );
        assert_eq!(
            encode_kind(&Optional {
                tag: Some(String::try_from("abc").expect("fits"))
            }),
            Err(ErrorKind::LimitExceeded)
        );
        assert_eq!(
            fails_after::<Optional>(b"\xa1\x63tag\x63abc", 64),
            Some((1, ErrorKind::LimitExceeded))
        );
    }
}
