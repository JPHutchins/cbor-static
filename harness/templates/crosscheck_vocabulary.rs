//! The field vocabulary at its longest encodings, alone, under the longest tag head,
//! self-described, and both: cbor-static writes minicbor's item, whole and in every output chunk
//! size, and borrowed views decode minicbor's item and write it back.

use core::mem::MaybeUninit;

use cbor_static::{
    ArrayRef, Cbor, DateTime, Decode, Encode, Encoder, Epoch, F16, MapRef, Progress, RawCbor,
    SelfDescribed, Tagged,
};
use minicbor::data::Tag;
use minicbor::encode::write::{Cursor, EndOfSlice};

type Oracle<'b> = minicbor::Encoder<Cursor<&'b mut [u8]>>;
type Built<'e, 'b> = Result<&'e mut Oracle<'b>, minicbor::encode::Error<EndOfSlice>>;

const TAG: u64 = u64::MAX;
const SELF_DESCRIBE: u64 = 55_799;
const POSITIVE_BIGNUM: u64 = 2;
const NEGATIVE_BIGNUM: u64 = 3;

#[derive(Cbor, Clone)]
struct Unit;

#[derive(Cbor, Clone)]
struct Newtype(u64);

#[derive(Cbor, Clone)]
struct TupleStruct(i64, Option<u128>);

#[derive(Cbor, Clone)]
struct Bytes(#[cbor(bytes)] [u8; 256]);

#[derive(Cbor, Clone)]
struct ByteVec(#[cbor(bytes)] heapless::Vec<u8, 256>);

#[test]
fn integers() {
    agree(&u8::MAX, |e| e.u8(u8::MAX));
    agree(&u16::MAX, |e| e.u16(u16::MAX));
    agree(&u32::MAX, |e| e.u32(u32::MAX));
    agree(&u64::MAX, |e| e.u64(u64::MAX));
    agree(&usize::MAX, |e| {
        e.u64(u64::try_from(usize::MAX).expect("a 64-bit host"))
    });
    agree(&u128::MAX, largest_bignum);
    agree(&i8::MIN, |e| e.i8(i8::MIN));
    agree(&i16::MIN, |e| e.i16(i16::MIN));
    agree(&i32::MIN, |e| e.i32(i32::MIN));
    agree(&i64::MIN, |e| e.i64(i64::MIN));
    agree(&isize::MIN, |e| {
        e.i64(i64::try_from(isize::MIN).expect("a 64-bit host"))
    });
    agree(&i128::MIN, smallest_bignum);
}

#[test]
fn other_scalars() {
    agree(&F16::from_bits(0x7bff), |e| e.f16(65_504.0));
    agree(&f32::MAX, |e| e.f32(f32::MAX));
    agree(&0.1f64, |e| e.f64(0.1));
    agree(&'\u{10ffff}', |e| e.str("\u{10ffff}"));
    agree(&true, |e| e.bool(true));
    agree(&(), |e| e.null());
}

#[test]
fn struct_shapes() {
    agree(&Unit, |e| e.null());
    agree(&Newtype(u64::MAX), |e| e.u64(u64::MAX));
    agree(&TupleStruct(i64::MIN, Some(u128::MAX)), |e| {
        largest_bignum(e.array(2)?.i64(i64::MIN)?)
    });
    agree(&Bytes([0xff; 256]), |e| e.bytes(&[0xff; 256]));
}

#[test]
fn bounded_containers() {
    let text = heapless::String::<256>::try_from("\u{10ffff}".repeat(64).as_str()).expect("fits");
    agree(&text, |e| e.str(&text));
    agree(
        &ByteVec(heapless::Vec::from_slice(&[0xff; 256]).expect("fits")),
        |e| e.bytes(&[0xff; 256]),
    );
    agree(
        &heapless::Vec::<u128, 3>::from_slice(&[u128::MAX; 3]).expect("fits"),
        |e| (0..3).try_fold(e.array(3)?, |e, _| largest_bignum(e)),
    );
    let map = (0..3)
        .map(|k| (u64::MAX - k, i128::MIN))
        .collect::<heapless::LinearMap<u64, i128, 3>>();
    agree(&map, |e| {
        map.keys()
            .try_fold(e.map(3)?, |e, key| smallest_bignum(e.u64(*key)?))
    });
}

#[test]
fn tags() {
    agree(&Tagged::<TAG, Tagged<TAG, u64>>(Tagged(u64::MAX)), |e| {
        e.tag(Tag::new(TAG))?.tag(Tag::new(TAG))?.u64(u64::MAX)
    });
    let epoch: Epoch<i128> = Tagged(i128::MIN);
    agree(&epoch, |e| smallest_bignum(e.tag(Tag::new(1))?));
    agree(&SelfDescribed(SelfDescribed(u128::MAX)), |e| {
        largest_bignum(
            e.tag(Tag::new(SELF_DESCRIBE))?
                .tag(Tag::new(SELF_DESCRIBE))?,
        )
    });
}

#[test]
fn borrowed_views() {
    decoded_again::<DateTime>(&wrapped(|e| {
        e.tag(Tag::new(0))?.str("2013-03-21T20:04:00Z")
    }));
    decoded_again::<RawCbor>(&wrapped(|e| largest_bignum(e.array(2)?.u64(u64::MAX)?)));
    decoded_again::<ArrayRef<u128>>(&wrapped(|e| {
        (0..3).try_fold(e.array(3)?, |e, _| largest_bignum(e))
    }));
    decoded_again::<MapRef<u64, i128>>(&wrapped(|e| {
        (0..3).try_fold(e.map(3)?, |e, k| smallest_bignum(e.u64(u64::MAX - k)?))
    }));
}

fn largest_bignum<'e, 'b>(e: &'e mut Oracle<'b>) -> Built<'e, 'b> {
    e.tag(Tag::new(POSITIVE_BIGNUM))?
        .bytes(&u128::MAX.to_be_bytes())
}

fn smallest_bignum<'e, 'b>(e: &'e mut Oracle<'b>) -> Built<'e, 'b> {
    e.tag(Tag::new(NEGATIVE_BIGNUM))?
        .bytes(&(!i128::MIN).to_be_bytes())
}

/// `value` is `oracle`'s item, alone and under each wrapper.
fn agree<T: Encode + Clone>(
    value: &T,
    oracle: impl for<'e, 'b> Fn(&'e mut Oracle<'b>) -> Built<'e, 'b>,
) {
    let [alone, tagged, described, both] = wrapped(oracle);
    written(value, &alone);
    written(&Tagged::<TAG, T>(value.clone()), &tagged);
    written(&SelfDescribed(value.clone()), &described);
    written(&SelfDescribed(Tagged::<TAG, T>(value.clone())), &both);
}

/// Each of `items` decodes as `T` under its wrapper and is written back as it was.
fn decoded_again<'de, T: Decode<'de> + Encode>(items: &'de [Vec<u8>; 4]) {
    let [alone, tagged, described, both] = items;
    rewritten::<T>(alone);
    rewritten::<Tagged<TAG, T>>(tagged);
    rewritten::<SelfDescribed<T>>(described);
    rewritten::<SelfDescribed<Tagged<TAG, T>>>(both);
}

fn rewritten<'de, T: Decode<'de> + Encode>(item: &'de [u8]) {
    let mut slot = MaybeUninit::<T>::uninit();
    written(
        cbor_static::decode_into(item, &mut slot).expect("cbor-static decodes minicbor's item"),
        item,
    );
}

/// `oracle`'s item alone, under the longest tag head, self-described, and both.
fn wrapped(oracle: impl for<'e, 'b> Fn(&'e mut Oracle<'b>) -> Built<'e, 'b>) -> [Vec<u8>; 4] {
    [
        item(&oracle),
        item(|e| oracle(e.tag(Tag::new(TAG))?)),
        item(|e| oracle(e.tag(Tag::new(SELF_DESCRIBE))?)),
        item(|e| oracle(e.tag(Tag::new(SELF_DESCRIBE))?.tag(Tag::new(TAG))?)),
    ]
}

fn item(oracle: impl for<'e, 'b> Fn(&'e mut Oracle<'b>) -> Built<'e, 'b>) -> Vec<u8> {
    let mut buffer = [0u8; 1024];
    let len = oracle(&mut minicbor::Encoder::new(Cursor::new(&mut buffer[..])))
        .expect("minicbor encodes")
        .writer()
        .position();
    buffer[..len].to_vec()
}

fn written<T: Encode>(value: &T, expected: &[u8]) {
    let mut whole = [0u8; 1024];
    assert_eq!(
        cbor_static::encode_into(value, &mut whole).map(|len| whole[..len].to_vec()),
        Ok(expected.to_vec()),
        "whole"
    );
    for chunk in 1..=expected.len() {
        assert_eq!(chunked(value, chunk), expected, "in chunks of {chunk}");
    }
}

fn chunked<T: Encode>(value: &T, chunk: usize) -> Vec<u8> {
    let mut encoder = Encoder::new(value);
    let mut buffer = vec![0u8; chunk];
    let mut out = Vec::new();
    loop {
        match encoder.resume(&mut buffer).expect("cbor-static encodes") {
            Progress::Done(n) => return [out, buffer[..n].to_vec()].concat(),
            Progress::NeedSpace(n) => out.extend_from_slice(&buffer[..n]),
        }
    }
}
