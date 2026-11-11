#![forbid(unsafe_code)]

use core::mem::MaybeUninit;

use cbor_static::{
    Cbor, Decode, Decoder, Encode, Encoder, ErrorKind, Fed, Progress, Tagged, decode_into,
    encode_into,
};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
enum Glue {
    A,
    B(u32),
    C { x: u32, y: bool },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
enum ReprC {
    A,
    B(u32),
    C { x: u32, y: bool },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum ReprU8 {
    A,
    B(u32),
    C { x: u32, y: bool },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Inner<'a> {
    #[serde(rename = "len")]
    length: u32,
    #[serde(rename = "buf")]
    buffer: &'a [u8],
    #[serde(rename = "rsn")]
    reason: &'a str,
    #[serde(rename = "confirmed", default, skip_serializing_if = "Option::is_none")]
    confirmed: Option<u32>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Outer<'a> {
    #[serde(rename = "ts")]
    kind: Glue,
    #[serde(rename = "inner")]
    inner: Inner<'a>,
    #[serde(rename = "msg")]
    message: i32,
    big: u64,
    flag: bool,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Owned {
    kind: Glue,
    repr: ReprC,
    count: u64,
    nested: Leaf,
    maybe: Option<u32>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Leaf {
    a: u32,
    b: i32,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Numbers {
    half: f64,
    single: f32,
    double: f64,
    time: Tagged<1, u64>,
    huge: u128,
    tiny: i128,
}

fn encode<T: Encode>(value: &T) -> Vec<u8> {
    let mut out = [0u8; 512];
    let len = encode_into(value, &mut out).expect("encodes");
    out[..len].to_vec()
}

fn decode<'de, T: Decode<'de> + Copy>(bytes: &'de [u8]) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    decode_into(bytes, &mut slot)
        .map(|value| *value)
        .map_err(|e| e.kind)
}

fn chunked_encode<T: Encode>(value: &T, chunk: usize) -> Vec<u8> {
    let mut encoder = Encoder::new(value);
    let mut buffer = vec![0u8; chunk];
    let mut out = Vec::new();
    loop {
        match encoder.resume(&mut buffer).expect("encodes") {
            Progress::Done(n) => return [out, buffer[..n].to_vec()].concat(),
            Progress::NeedSpace(n) => out.extend_from_slice(&buffer[..n]),
        }
    }
}

fn chunked_decode<'de, T: Decode<'de> + Copy>(
    bytes: &'de [u8],
    chunk: usize,
) -> Result<T, ErrorKind> {
    let mut slot = MaybeUninit::uninit();
    let mut decoder = Decoder::new(&mut slot);
    for piece in bytes.chunks(chunk) {
        if let Fed::Done(_) = decoder.feed(piece).map_err(|e| e.kind)? {
            break;
        }
    }
    decoder.finish().map(|value| *value).map_err(|e| e.kind)
}

#[test]
fn every_enum_path_writes_the_same_externally_tagged_bytes() {
    let golden: [&[u8]; 3] = [
        b"\x61A",
        b"\xa1\x61B\x18\x2a",
        b"\xa1\x61C\xa2\x61x\x07\x61y\xf4",
    ];
    let glue = [Glue::A, Glue::B(42), Glue::C { x: 7, y: false }];
    let repr_c = [ReprC::A, ReprC::B(42), ReprC::C { x: 7, y: false }];
    let repr_u8 = [ReprU8::A, ReprU8::B(42), ReprU8::C { x: 7, y: false }];
    for (index, bytes) in golden.iter().enumerate() {
        assert_eq!(encode(&glue[index]), *bytes);
        assert_eq!(encode(&repr_c[index]), *bytes);
        assert_eq!(encode(&repr_u8[index]), *bytes);
        assert_eq!(decode::<Glue>(bytes), Ok(glue[index]));
        assert_eq!(decode::<ReprC>(bytes), Ok(repr_c[index]));
        assert_eq!(decode::<ReprU8>(bytes), Ok(repr_u8[index]));
    }
}

#[test]
fn borrowed_nested_struct_round_trips() {
    let value = Outer {
        kind: Glue::C { x: 1, y: true },
        inner: Inner {
            length: 300,
            buffer: &[1, 2, 3],
            reason: "hello",
            confirmed: None,
        },
        message: -5,
        big: 1 << 40,
        flag: true,
    };
    let bytes = encode(&value);
    assert_eq!(
        bytes,
        [
            b"\xa5\x62ts\xa1\x61C\xa2\x61x\x01\x61y\xf5".as_slice(),
            b"\x65inner\xa3\x63len\x19\x01\x2c\x63buf\x43\x01\x02\x03\x63rsn\x65hello",
            b"\x63msg\x24\x63big\x1b\x00\x00\x01\x00\x00\x00\x00\x00\x64flag\xf5",
        ]
        .concat()
    );
    assert_eq!(decode::<Outer>(&bytes), Ok(value));
}

#[test]
fn chunked_encoding_matches_one_shot_for_every_chunk_size() {
    let value = Outer {
        kind: Glue::B(70_000),
        inner: Inner {
            length: 1,
            buffer: &[9; 40],
            reason: "a key that spans many chunks",
            confirmed: Some(3),
        },
        message: i32::MIN,
        big: u64::MAX,
        flag: false,
    };
    let whole = encode(&value);
    for chunk in 1..=64 {
        assert_eq!(chunked_encode(&value, chunk), whole, "chunk {chunk}");
    }
}

#[test]
fn chunked_decoding_matches_one_shot_for_owned_targets() {
    let value = Owned {
        kind: Glue::C {
            x: 1 << 20,
            y: true,
        },
        repr: ReprC::B(u32::MAX),
        count: u64::MAX - 1,
        nested: Leaf { a: 24, b: -25 },
        maybe: Some(0),
    };
    let unknown = b"\x6bunknown key\xbf\x61a\x9f\x01\x7f\x62bc\xff\xff\xff";
    let bytes = [&encode(&value)[..1], unknown, &encode(&value)[1..]].concat();
    let bytes = [&[0xa6][..], &bytes[1..]].concat();
    for chunk in 1..=64 {
        assert_eq!(
            chunked_decode::<Owned>(&bytes, chunk),
            Ok(value),
            "chunk {chunk}"
        );
    }
}

#[test]
fn borrowed_payloads_need_contiguous_input() {
    let bytes = encode(&Inner {
        length: 1,
        buffer: &[1, 2, 3, 4],
        reason: "x",
        confirmed: None,
    });
    assert_eq!(
        chunked_decode::<Inner>(&bytes, 6),
        Err(ErrorKind::NeedContiguous)
    );
}

#[test]
fn absent_options_are_omitted_and_null_or_missing_decode_as_none() {
    let leaf_only = b"\xa2\x61a\x01\x61b\x02";
    assert_eq!(decode::<Leaf>(leaf_only), Ok(Leaf { a: 1, b: 2 }));
    let inner = b"\xa3\x63len\x01\x63buf\x40\x63rsn\x60";
    let with_null = b"\xa4\x63len\x01\x63buf\x40\x63rsn\x60\x69confirmed\xf6";
    let expected = Inner {
        length: 1,
        buffer: &[],
        reason: "",
        confirmed: None,
    };
    assert_eq!(decode::<Inner>(inner), Ok(expected));
    assert_eq!(decode::<Inner>(with_null), Ok(expected));
    assert_eq!(encode(&expected), inner);
}

#[test]
fn decode_errors_are_reported_with_their_kind() {
    let cases: [(&[u8], ErrorKind); 9] = [
        (b"\xa2\x61a\x01\x61a\x02", ErrorKind::DuplicateKey),
        (b"\xa1\x61a\x01", ErrorKind::MissingField),
        (
            b"\xa2\x61a\x01\x61b\x1b\x00\x00\x00\x01\x00\x00\x00\x00",
            ErrorKind::OutOfRange,
        ),
        (b"\xa2\x61a\x20\x61b\x00", ErrorKind::OutOfRange),
        (b"\xa2\x61a\xf5\x61b\x00", ErrorKind::TypeMismatch),
        (b"\xa2\x61a\x01\x61b", ErrorKind::UnexpectedEnd),
        (b"\xa2\x61a\x01\x61b\x00\x00", ErrorKind::TrailingInput),
        (
            b"\xa3\x61a\x01\x61b\x00\x62\xc3\x28\x00",
            ErrorKind::InvalidUtf8,
        ),
        (b"\xa2\x61a\x01\x61b\x1c", ErrorKind::NotWellFormed),
    ];
    for (bytes, kind) in cases {
        assert_eq!(decode::<Leaf>(bytes), Err(kind), "{bytes:x?}");
    }
    assert_eq!(decode::<Glue>(b"\x61D"), Err(ErrorKind::UnknownVariant));
    assert_eq!(
        decode::<Glue>(b"\xa2\x61B\x01\x61A\x02"),
        Err(ErrorKind::TypeMismatch)
    );
}

#[test]
fn floats_tags_and_bignums_round_trip() {
    let value = Numbers {
        half: 1.5,
        single: 100_000.0,
        double: 1.1,
        time: Tagged(1_363_896_240),
        huge: u128::MAX,
        tiny: i128::MIN,
    };
    let bytes = encode(&value);
    assert_eq!(
        bytes,
        [
            b"\xa6\x64half\xf9\x3e\x00\x66single\xfa\x47\xc3\x50\x00".as_slice(),
            b"\x66double\xfb\x3f\xf1\x99\x99\x99\x99\x99\x9a\x64time\xc1\x1a\x51\x4b\x67\xb0",
            b"\x64huge\xc2\x50",
            &[0xff; 16],
            b"\x64tiny\xc3\x50\x7f",
            &[0xff; 15],
        ]
        .concat()
    );
    assert_eq!(decode::<Numbers>(&bytes), Ok(value));
    assert_eq!(
        decode::<u128>(b"\x1b\xff\xff\xff\xff\xff\xff\xff\xff"),
        Ok(u128::from(u64::MAX))
    );
    assert_eq!(decode::<i128>(b"\x38\x63"), Ok(-100));
    assert_eq!(
        decode::<Tagged<1, u64>>(b"\xc2\x00"),
        Err(ErrorKind::UnexpectedTag)
    );
}
