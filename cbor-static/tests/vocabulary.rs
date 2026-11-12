//! R-126: a round trip for every type of the field vocabulary (D-20) this phase covers. Each value
//! encodes, decodes back to itself whole and in every input chunk size the type allows, and
//! encodes again to the same bytes. The other test files pin each type's bytes and errors.
#![cfg_attr(feature = "nightly", feature(f16))]

mod common;

use core::fmt::Debug;
use core::mem::MaybeUninit;

use cbor_static::{
    ArrayRef, Cbor, Chunks, DateTime, Decode, Encode, Epoch, F16, MapRef, RawCbor, SelfDescribed,
    Tagged,
};
use common::{encode, golden_in_place};

/// An owned value: compared where it was decoded, whole and in every chunk size.
fn owned<T>(value: &T)
where
    T: Encode + for<'de> Decode<'de> + PartialEq + Debug,
{
    golden_in_place(value, &encode(value));
}

/// An owned value decoded whole: a 128-bit integer beyond 64 bits is a bignum, whose digits need
/// contiguous input (as #11 pinned).
fn whole<T>(value: &T)
where
    T: Encode + for<'de> Decode<'de> + PartialEq + Debug,
{
    let bytes = encode(value);
    let mut slot = MaybeUninit::<T>::uninit();
    assert_eq!(
        cbor_static::decode_into(&bytes, &mut slot).map(|decoded| &*decoded == value),
        Ok(true)
    );
}

/// A borrowing value, which needs contiguous input: decoded whole, it encodes to the same bytes.
fn borrowed<'de, T: Encode + Decode<'de>>(bytes: &'de [u8]) {
    let mut slot = MaybeUninit::<T>::uninit();
    let decoded = cbor_static::decode_into(bytes, &mut slot).expect("decodes");
    assert_eq!(encode(&*decoded), bytes, "{bytes:x?}");
}

#[derive(Cbor, Debug, PartialEq)]
struct Unit;

#[derive(Cbor, Debug, PartialEq)]
struct Newtype(u32);

#[derive(Cbor, Debug, PartialEq)]
struct TupleStruct(u8, Option<i16>);

#[derive(Cbor, Debug, PartialEq)]
struct Bytes {
    #[cbor(bytes)]
    fixed: [u8; 3],
}

#[test]
fn every_integer_width() {
    owned(&u8::MAX);
    owned(&u16::MAX);
    owned(&u32::MAX);
    owned(&u64::MAX);
    owned(&u128::from(u64::MAX));
    whole(&u128::MAX);
    owned(&usize::MAX);
    owned(&i8::MIN);
    owned(&i16::MIN);
    owned(&i32::MIN);
    owned(&i64::MIN);
    owned(&i128::from(i64::MIN));
    whole(&i128::MIN);
    owned(&isize::MIN);
}

#[test]
fn every_other_scalar() {
    owned(&true);
    owned(&1.5f32);
    owned(&-0.0f64);
    owned(&F16::from_bits(0x3e00));
    owned(&'€');
    owned(&());
}

#[cfg(feature = "nightly")]
#[test]
fn the_f16_primitive() {
    owned(&1.5f16);
}

#[test]
fn every_struct_shape() {
    owned(&Unit);
    owned(&Newtype(7));
    owned(&TupleStruct(1, None));
    owned(&Bytes { fixed: [1, 2, 3] });
}

#[test]
fn containers_and_tags() {
    owned(&Some(Newtype(1)));
    owned(&[1u16, 2, 3]);
    owned(&(1u8, true, 'x'));
    owned(&Tagged::<1000, Newtype>(Newtype(2)));
    let epoch: Epoch<u64> = Tagged(1_363_896_240);
    owned(&epoch);
    owned(&SelfDescribed(Unit));
}

#[test]
fn borrowed_views() {
    borrowed::<&str>(b"\x63abc");
    borrowed::<&[u8]>(b"\x42\x01\x02");
    borrowed::<Chunks<str>>(b"\x63abc");
    borrowed::<DateTime>(b"\xc0\x6a2013-03-21");
    borrowed::<RawCbor>(b"\x82\x18\x01\x9f\xff");
    borrowed::<ArrayRef<u16>>(b"\x82\x01\x19\x01\x00");
    borrowed::<MapRef<&str, bool>>(b"\xa1\x61a\xf5");
}

#[cfg(feature = "heapless")]
#[test]
fn heapless_containers() {
    #[derive(Cbor, Debug, PartialEq)]
    struct Frame {
        #[cbor(bytes)]
        payload: heapless::Vec<u8, 4>,
    }

    owned(&heapless::String::<8>::try_from("héllo").expect("fits"));
    owned(&heapless::Vec::<u32, 4, u8>::from_slice(&[1, 2]).expect("fits"));
    owned(&Frame {
        payload: heapless::Vec::from_slice(&[9, 8]).expect("fits"),
    });
    owned(
        &[(1u8, true), (2, false)]
            .into_iter()
            .collect::<heapless::LinearMap<u8, bool, 4>>(),
    );
}
