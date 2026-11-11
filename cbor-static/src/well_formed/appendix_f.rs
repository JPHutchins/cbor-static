//! R-102: every example of RFC 8949 Appendix F.1, grouped as the RFC groups them, and every
//! reserved two-byte simple value `f8 00..1f`.

use std::vec;
use std::vec::Vec;

use super::{Class, oracle, skipped, streamed, tokens};

const HEAD_CUT_SHORT: [&[u8]; 18] = [
    b"\x18",
    b"\x19",
    b"\x1a",
    b"\x1b",
    b"\x19\x01",
    b"\x1a\x01\x02",
    b"\x1b\x01\x02\x03\x04\x05\x06\x07",
    b"\x38",
    b"\x58",
    b"\x78",
    b"\x98",
    b"\x9a\x01\xff\x00",
    b"\xb8",
    b"\xd8",
    b"\xf8",
    b"\xf9\x00",
    b"\xfa\x00\x00",
    b"\xfb\x00\x00\x00",
];

const STRING_CUT_SHORT: [&[u8]; 6] = [
    b"\x41",
    b"\x61",
    b"\x5a\xff\xff\xff\xff\x00",
    b"\x5b\xff\xff\xff\xff\xff\xff\xff\xff\x01\x02\x03",
    b"\x7a\xff\xff\xff\xff\x00",
    b"\x7b\x7f\xff\xff\xff\xff\xff\xff\xff\x01\x02\x03",
];

const CONTAINER_SHORT_OF_ITEMS: [&[u8]; 7] = [
    b"\x81",
    b"\x81\x81\x81\x81\x81\x81\x81\x81\x81",
    b"\x82\x00",
    b"\xa1",
    b"\xa2\x01\x02",
    b"\xa1\x00",
    b"\xa2\x00\x00\x00",
];

const TAG_WITHOUT_CONTENT: [&[u8]; 1] = [b"\xc0"];

const STRING_NOT_CLOSED: [&[u8]; 2] = [b"\x5f\x41\x00", b"\x7f\x61\x00"];

const CONTAINER_NOT_CLOSED: [&[u8]; 8] = [
    b"\x9f",
    b"\x9f\x01\x02",
    b"\xbf",
    b"\xbf\x01\x02\x01\x02",
    b"\x81\x9f",
    b"\x9f\x80\x00",
    b"\x9f\x9f\x9f\x9f\x9f\xff\xff\xff\xff",
    b"\x9f\x81\x9f\x81\x9f\x9f\xff\xff\xff",
];

const RESERVED_ADDITIONAL_INFORMATION: [&[u8]; 24] = [
    b"\x1c", b"\x1d", b"\x1e", b"\x3c", b"\x3d", b"\x3e", b"\x5c", b"\x5d", b"\x5e", b"\x7c",
    b"\x7d", b"\x7e", b"\x9c", b"\x9d", b"\x9e", b"\xbc", b"\xbd", b"\xbe", b"\xdc", b"\xdd",
    b"\xde", b"\xfc", b"\xfd", b"\xfe",
];

const RESERVED_TWO_BYTE_SIMPLE: [&[u8]; 4] = [b"\xf8\x00", b"\xf8\x01", b"\xf8\x18", b"\xf8\x1f"];

const CHUNK_OF_ANOTHER_TYPE: [&[u8]; 8] = [
    b"\x5f\x00\xff",
    b"\x5f\x21\xff",
    b"\x5f\x61\x00\xff",
    b"\x5f\x80\xff",
    b"\x5f\xa0\xff",
    b"\x5f\xc0\x00\xff",
    b"\x5f\xe0\xff",
    b"\x7f\x41\x00\xff",
];

const CHUNK_OF_INDEFINITE_LENGTH: [&[u8]; 2] =
    [b"\x5f\x5f\x41\x00\xff\xff", b"\x7f\x7f\x61\x00\xff\xff"];

const BREAK_ALONE: [&[u8]; 1] = [b"\xff"];

const BREAK_IN_DEFINITE: [&[u8]; 8] = [
    b"\x81\xff",
    b"\x82\x00\xff",
    b"\xa1\xff",
    b"\xa1\xff\x00",
    b"\xa1\x00\xff",
    b"\xa2\x00\x00\xff",
    b"\x9f\x81\xff",
    b"\x9f\x82\x9f\x81\x9f\x9f\xff\xff\xff\xff",
];

const BREAK_IN_VALUE_POSITION: [&[u8]; 2] = [b"\xbf\x00\xff", b"\xbf\x00\x00\x00\xff"];

const INDEFINITE_INTEGER_OR_TAG: [&[u8]; 3] = [b"\x1f", b"\x3f", b"\xdf"];

fn vectors() -> Vec<(Vec<u8>, Class)> {
    [
        (&HEAD_CUT_SHORT[..], Class::Truncated),
        (&STRING_CUT_SHORT[..], Class::Truncated),
        (&CONTAINER_SHORT_OF_ITEMS[..], Class::Truncated),
        (&TAG_WITHOUT_CONTENT[..], Class::Truncated),
        (&STRING_NOT_CLOSED[..], Class::Truncated),
        (&CONTAINER_NOT_CLOSED[..], Class::Truncated),
        (&RESERVED_ADDITIONAL_INFORMATION[..], Class::Malformed),
        (&RESERVED_TWO_BYTE_SIMPLE[..], Class::Malformed),
        (&CHUNK_OF_ANOTHER_TYPE[..], Class::Malformed),
        (&CHUNK_OF_INDEFINITE_LENGTH[..], Class::Malformed),
        (&BREAK_ALONE[..], Class::Malformed),
        (&BREAK_IN_DEFINITE[..], Class::Malformed),
        (&BREAK_IN_VALUE_POSITION[..], Class::Malformed),
        (&INDEFINITE_INTEGER_OR_TAG[..], Class::Malformed),
    ]
    .into_iter()
    .flat_map(|(group, class)| group.iter().map(move |bytes| (bytes.to_vec(), class)))
    .chain((0..0x20).map(|simple| (vec![0xf8, simple], Class::Malformed)))
    .collect()
}

#[test]
fn every_appendix_f_example_fails_with_its_kind_in_every_walker() {
    for (bytes, class) in vectors() {
        assert_eq!(oracle::item(&bytes), Err(class), "reference {bytes:02x?}");
        assert_eq!(tokens(&bytes), Err(class), "tokens {bytes:02x?}");
        assert_eq!(skipped(&bytes), Err(class), "skip {bytes:02x?}");
        for chunk in 1..=bytes.len() + 3 {
            assert_eq!(
                streamed(&bytes, chunk).map_err(Class::from),
                Err(class),
                "stream in chunks of {chunk}: {bytes:02x?}"
            );
        }
    }
}
