#![forbid(unsafe_code)]
//! Generic structs and enums (R-127): each type parameter is bounded by the codec trait, as serde
//! bounds it, and the layout is built inside each impl, since the offsets depend on the
//! parameters. Exercised with #12's vocabulary.

mod common;

use cbor_static::{Cbor, F16};
use common::{decode, encode, golden};

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Wrapper<T> {
    value: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    maybe: Option<T>,
    pair: (T, u8),
    both: [T; 2],
}

#[test]
fn a_generic_struct_derives_for_each_vocabulary_type() {
    golden(
        &Wrapper {
            value: 1u32,
            maybe: None,
            pair: (2, 3),
            both: [4, 5],
        },
        b"\xa3\x65value\x01\x64pair\x82\x02\x03\x64both\x82\x04\x05",
    );
    golden(
        &Wrapper {
            value: true,
            maybe: Some(false),
            pair: (true, 0),
            both: [false, true],
        },
        b"\xa4\x65value\xf5\x65maybe\xf4\x64pair\x82\xf5\x00\x64both\x82\xf4\xf5",
    );
    golden(
        &Wrapper {
            value: F16::from_bits(0x3c00),
            maybe: None,
            pair: (F16::from_bits(0), 1),
            both: [F16::from_bits(0x3c00); 2],
        },
        b"\xa3\x65value\xf9\x3c\x00\x64pair\x82\xf9\x00\x00\x01\x64both\x82\xf9\x3c\x00\xf9\x3c\x00",
    );
    golden(
        &Wrapper {
            value: Wrapper {
                value: -1i8,
                maybe: None,
                pair: (0, 0),
                both: [0, 0],
            },
            maybe: None,
            pair: (
                Wrapper {
                    value: 0,
                    maybe: Some(1),
                    pair: (0, 0),
                    both: [0, 0],
                },
                9,
            ),
            both: [Wrapper {
                value: 0,
                maybe: None,
                pair: (0, 0),
                both: [0, 0],
            }; 2],
        },
        &encode(&Wrapper {
            value: Wrapper {
                value: -1i8,
                maybe: None,
                pair: (0, 0),
                both: [0, 0],
            },
            maybe: None,
            pair: (
                Wrapper {
                    value: 0,
                    maybe: Some(1),
                    pair: (0, 0),
                    both: [0, 0],
                },
                9,
            ),
            both: [Wrapper {
                value: 0,
                maybe: None,
                pair: (0, 0),
                both: [0, 0],
            }; 2],
        }),
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Named<'a, T> {
    name: &'a str,
    value: T,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Buffer<const N: usize> {
    #[cbor(bytes)]
    data: [u8; N],
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Pair<A, B>(A, B);

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Id<T>(T);

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Bounded<T: Copy>
where
    T: PartialEq,
{
    value: T,
}

fn seven() -> u8 {
    7
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Defaulted<T> {
    value: T,
    #[serde(default = "seven")]
    seven: u8,
}

#[test]
fn lifetimes_const_parameters_bounds_and_defaults_go_with_type_parameters() {
    let named = Named {
        name: "n",
        value: 5u16,
    };
    assert_eq!(encode(&named), b"\xa2\x64name\x61n\x65value\x05");
    assert_eq!(
        decode::<Named<u16>>(b"\xa2\x64name\x61n\x65value\x05"),
        Ok(named)
    );
    golden(&Buffer { data: [1, 2, 3] }, b"\xa1\x64data\x43\x01\x02\x03");
    golden(&Pair(1u8, true), b"\x82\x01\xf5");
    golden(&Id(Pair(-1i32, 2u64)), b"\x82\x20\x02");
    golden(&Bounded { value: 3u8 }, b"\xa1\x65value\x03");
    assert_eq!(
        decode::<Defaulted<u8>>(b"\xa1\x65value\x01"),
        Ok(Defaulted { value: 1, seven: 7 })
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
enum Either<L, R> {
    Left(L),
    Right(R),
    Both { left: L, right: R },
    Neither,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Tagged<T> {
    One(T),
    Two { first: T, second: u8 },
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
#[cbor(index)]
enum Indexed<T> {
    Unit,
    Payload(T),
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
enum Value<T> {
    Int(i64),
    Other(T),
}

#[test]
fn generic_enums_derive_through_glue_and_through_tables() {
    golden(&Either::<u8, bool>::Left(1), b"\xa1\x64Left\x01");
    golden(&Either::<u8, bool>::Right(true), b"\xa1\x65Right\xf5");
    golden(
        &Either::Both {
            left: 1u8,
            right: false,
        },
        b"\xa1\x64Both\xa2\x64left\x01\x65right\xf4",
    );
    golden(&Either::<u8, bool>::Neither, b"\x67Neither");
    golden(&Tagged::One(-2i16), b"\xa1\x63One\x21");
    golden(
        &Tagged::Two {
            first: 3i16,
            second: 4,
        },
        b"\xa1\x63Two\xa2\x65first\x03\x66second\x04",
    );
    golden(&Indexed::<u32>::Unit, b"\x00");
    golden(&Indexed::Payload(500u32), b"\x82\x01\x19\x01\xf4");
    golden(&Value::<bool>::Int(-3), b"\x22");
    golden(&Value::Other(true), b"\xf5");
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct Meta<T> {
    d: u8,
    b: T,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct Outer<T> {
    c: u8,
    #[serde(flatten)]
    meta: Meta<T>,
    a: u8,
}

#[test]
fn a_generic_struct_flattens_into_a_deterministic_parent_sorted_among_its_keys() {
    golden(
        &Outer {
            c: 3,
            meta: Meta { d: 4, b: 2u16 },
            a: 1,
        },
        b"\xa4\x61a\x01\x61b\x02\x61c\x03\x61d\x04",
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Leaf<'b, U> {
    tag: &'b str,
    value: U,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Middle<'a> {
    mid: u8,
    #[serde(flatten)]
    leaf: Leaf<'a, u16>,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Top<'a, T> {
    own: T,
    #[serde(flatten)]
    middle: Middle<'a>,
}

#[test]
fn a_generic_parent_flattens_a_child_that_flattens_a_generic_grandchild() {
    let top = Top {
        own: true,
        middle: Middle {
            mid: 1,
            leaf: Leaf {
                tag: "t",
                value: 300,
            },
        },
    };
    let bytes = b"\xa4\x63own\xf5\x63mid\x01\x63tag\x61t\x65value\x19\x01\x2c";
    assert_eq!(encode(&top), bytes);
    assert_eq!(decode::<Top<bool>>(bytes), Ok(top));
}

#[cfg(feature = "heapless")]
mod bounded {
    use cbor_static::Cbor;

    use super::common::golden_in_place;

    #[derive(Cbor, Debug, PartialEq)]
    struct Bag<T, const N: usize> {
        items: heapless::Vec<T, N>,
    }

    #[test]
    fn a_generic_struct_holds_a_generic_container() {
        golden_in_place(
            &Bag::<i8, 3> {
                items: [-1, 0, 1].into_iter().collect(),
            },
            b"\xa1\x65items\x83\x20\x00\x01",
        );
    }
}
