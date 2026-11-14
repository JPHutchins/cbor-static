#![forbid(unsafe_code)]
//! Deterministic encoding (D-11b, R-125): keys sorted at compile time in RFC 8949 §4.2.1's
//! bytewise order and §4.2.3's length-first order, and NaN canonicalised by the root's ops.

mod common;

use cbor_static::{Cbor, Encode, ErrorKind, F16};
use common::{decode_strict, encode, golden};

/// RFC 8949's example keys that a struct can have, declared out of order.
macro_rules! keyed {
    ($name:ident $(, $order:ident)?) => {
        #[derive(Cbor, Debug, PartialEq, Clone, Copy)]
        $(#[cbor($order)])?
        struct $name {
            #[cbor(key = "aa")]
            aa: u8,
            #[cbor(key = -1)]
            minus_one: u8,
            #[cbor(key = "z")]
            z: u8,
            #[cbor(key = 100)]
            hundred: u8,
            #[cbor(key = 10)]
            ten: u8,
        }

        impl $name {
            const VALUE: Self = Self {
                aa: 1,
                minus_one: 2,
                z: 3,
                hundred: 4,
                ten: 5,
            };
        }
    };
}

keyed!(Declared);
keyed!(Bytewise, deterministic);
keyed!(LengthFirst, length_first);

#[test]
fn struct_keys_sort_at_compile_time_as_rfc_8949_lists_them() {
    golden(
        &Declared::VALUE,
        b"\xa5\x62aa\x01\x20\x02\x61z\x03\x18\x64\x04\x0a\x05",
    );
    golden(
        &Bytewise::VALUE,
        b"\xa5\x0a\x05\x18\x64\x04\x20\x02\x61z\x03\x62aa\x01",
    );
    golden(
        &LengthFirst::VALUE,
        b"\xa5\x0a\x05\x20\x02\x18\x64\x04\x61z\x03\x62aa\x01",
    );
}

#[test]
fn a_bytewise_encoding_passes_strict_and_a_declared_one_does_not() {
    assert_eq!(
        decode_strict::<Bytewise>(&encode(&Bytewise::VALUE)),
        Ok(Bytewise::VALUE)
    );
    assert_eq!(
        decode_strict::<Declared>(&encode(&Declared::VALUE)),
        Err(ErrorKind::NotDeterministic)
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Floats {
    single: f32,
    double: f64,
    half: F16,
    zero: f64,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct CanonicalFloats {
    single: f32,
    double: f64,
    half: F16,
    zero: f64,
}

#[test]
fn a_deterministic_root_writes_one_nan_and_keeps_negative_zero() {
    let (single, double, half, zero) = (
        f32::from_bits(0x7fc0_0001),
        f64::from_bits(0x7ff8_0000_0000_0001),
        F16::from_bits(0x7e01),
        -0.0,
    );
    assert_eq!(
        encode(&Floats {
            single,
            double,
            half,
            zero
        }),
        b"\xa4\x66single\xfa\x7f\xc0\x00\x01\x66double\xfb\x7f\xf8\x00\x00\x00\x00\x00\x01\x64half\xf9\x7e\x01\x64zero\xf9\x80\x00"
    );
    assert_eq!(
        encode(&CanonicalFloats {
            single,
            double,
            half,
            zero
        }),
        b"\xa4\x64half\xf9\x7e\x00\x64zero\xf9\x80\x00\x66double\xf9\x7e\x00\x66single\xf9\x7e\x00"
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(index)]
enum Op {
    Read = 0,
    Write = 2,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
struct Pair(u8, Op);

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct Inner {
    b: u8,
    a: u8,
}

/// Each part writes no map of its own, or sorts its keys the same way.
#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
struct Outer {
    op: Op,
    pair: Pair,
    inner: Option<Inner>,
    inners: [Inner; 1],
    tuple: (u8, Inner),
}

#[test]
fn a_deterministic_type_holds_map_free_and_deterministic_parts() {
    let inner = Inner { b: 2, a: 1 };
    golden(
        &Outer {
            op: Op::Write,
            pair: Pair(7, Op::Read),
            inner: Some(inner),
            inners: [inner],
            tuple: (9, inner),
        },
        b"\xa5\x62op\x02\x64pair\x82\x07\x00\x65inner\xa2\x61a\x01\x61b\x02\x65tuple\x82\x09\xa2\x61a\x01\x61b\x02\x66inners\x81\xa2\x61a\x01\x61b\x02",
    );
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[cbor(deterministic)]
enum Shape {
    Circle(u8),
    Rect { w: u8, h: u8 },
    Empty,
}

#[derive(Cbor, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
#[cbor(length_first)]
enum Table {
    Point {
        #[cbor(key = 100)]
        far: u8,
        #[cbor(key = -1)]
        near: u8,
    },
}

#[test]
fn struct_variants_sort_their_fields_with_glue_and_with_tables() {
    golden(&Shape::Circle(3), b"\xa1\x66Circle\x03");
    golden(
        &Shape::Rect { w: 4, h: 5 },
        b"\xa1\x64Rect\xa2\x61h\x05\x61w\x04",
    );
    golden(&Shape::Empty, b"\x65Empty");
    golden(
        &Table::Point { far: 1, near: 2 },
        b"\xa1\x65Point\xa2\x20\x02\x18\x64\x01",
    );
}

#[test]
fn only_a_deterministic_root_selects_the_ops() {
    assert!(<Bytewise as Encode>::DETERMINISTIC.is_some());
    assert!(<Declared as Encode>::DETERMINISTIC.is_none());
}
