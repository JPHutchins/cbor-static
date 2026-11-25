//! Each row of the normalization table, pinned on the input that shows it, through every oracle;
//! and Python's key equality, positive and negative for each equality it has.

use crate::common::differential::legs::{Refusal, Row, agree, for_ciborium};
use crate::common::differential::python::{collapsed, equal};
use crate::common::differential::readers::tokens;
use crate::common::differential::{Data, Float, Length};

fn bytes(hex: &str) -> Vec<u8> {
    crate::common::differential::hex::decoded(hex).expect("hex")
}

fn rows(hex: &str) -> Vec<Row> {
    agree(&bytes(hex))
}

fn ours(hex: &str) -> Data {
    tokens(&bytes(hex)).expect("Tokens reads it")
}

fn int(value: i128) -> Data {
    Data::Integer(value)
}

fn float(value: f64) -> Data {
    Data::Float(Float::read(value))
}

fn map(entries: Vec<(Data, Data)>) -> Data {
    Data::Map(entries, Length::Definite)
}

fn array(items: Vec<Data>) -> Data {
    Data::Array(items, Length::Definite)
}

/// One test per input: the rows `agree` reaches on it through every oracle, and the rows it
/// does not.
macro_rules! rows {
    ($($test:ident: $hex:literal reaches [$($row:ident),*] misses [$($missed:ident),*];)*) => {
        $(
            #[test]
            fn $test() {
                let reached = rows($hex);
                $(assert!(reached.contains(&Row::$row), "{row:?}", row = Row::$row);)*
                $(assert!(!reached.contains(&Row::$missed), "{row:?}", row = Row::$missed);)*
            }
        )*
    };
}

/// One test per input: what ciborium's leg expects of it.
macro_rules! for_ciborium {
    ($($test:ident: $hex:literal => $expected:expr;)*) => {
        $(
            #[test]
            fn $test() {
                assert_eq!(for_ciborium(ours($hex)), $expected);
            }
        )*
    };
}

mod undefined_is_null_to_ciborium_only {
    use super::{Row, rows};

    rows! {
        undefined: "f7" reaches [CiboriumUndefinedIsNull] misses [];
        null: "f6" reaches [] misses [CiboriumUndefinedIsNull];
    }
}

mod ciborium_refuses_every_unassigned_simple_value {
    use super::{Row, rows};

    rows! {
        simple_0: "e0" reaches [CiboriumRefusesUnassignedSimple] misses [];
        simple_19: "f3" reaches [CiboriumRefusesUnassignedSimple] misses [];
        simple_32: "f820" reaches [CiboriumRefusesUnassignedSimple] misses [];
        simple_255: "f8ff" reaches [CiboriumRefusesUnassignedSimple] misses [];
        in_an_array: "8201e0" reaches [CiboriumRefusesUnassignedSimple] misses [];
        false_is_assigned: "f4" reaches [] misses [CiboriumRefusesUnassignedSimple];
    }
}

mod ciborium_reads_a_short_bignum_as_its_integer {
    use super::{Data, Refusal, Row, for_ciborium, int, ours, rows};

    for_ciborium! {
        one: "c24101" => Ok(int(1));
        minus_two: "c34101" => Ok(int(-2));
        no_bytes: "c240" => Ok(int(0));
        a_zero_byte: "c24100" => Ok(int(0));
        past_a_u64: "c34f00ffffffffffffffffffffffffffff" => Ok(Data::Tag(3, Box::new(Data::Bytes(vec![0xff; 14]))));
        below_i128_min: "c35080000000000000000000000000000000" => Err::<Data, _>(Refusal::BignumTooLarge);
    }

    rows! {
        reached_one: "c24101" reaches [CiboriumBignum] misses [];
        reached_minus_two: "c34101" reaches [CiboriumBignum] misses [];
        reached_no_bytes: "c240" reaches [CiboriumBignum] misses [];
        reached_16_bytes: "c250ffffffffffffffffffffffffffffffff" reaches [CiboriumBignum] misses [];
    }
}

mod ciborium_keeps_a_bignum_it_cannot_read_as_a_tag {
    use super::{Row, rows};

    rows! {
        seventeen_bytes: "c25100000000000000000000000000000000ff" reaches [] misses [CiboriumBignum];
        chunked: "c25f4101ff" reaches [] misses [CiboriumBignum];
        over_an_array: "c2820102" reaches [] misses [CiboriumBignum];
    }
}

mod ciborium_refuses_a_negative_bignum_below_i128_min {
    use super::{Row, rows};

    rows! {
        below: "c35080000000000000000000000000000000" reaches [CiboriumRefusesBignum] misses [];
        at_i128_min: "c3507fffffffffffffffffffffffffffffff" reaches [] misses [CiboriumRefusesBignum];
    }
}

mod ciborium_refuses_for_the_first_reason_it_reads {
    use super::{Data, Refusal, Row, for_ciborium, ours, rows};

    for_ciborium! {
        bignum_first: "bfc359001080000000000000000000000000000000f84fff" => Err::<Data, _>(Refusal::BignumTooLarge);
        simple_first: "bff84fc359001080000000000000000000000000000000ff" => Err::<Data, _>(Refusal::UnassignedSimple);
    }

    rows! {
        reached_bignum_first: "bfc359001080000000000000000000000000000000f84fff" reaches [CiboriumRefusesBignum] misses [CiboriumRefusesUnassignedSimple];
    }
}

mod nans_lose_sign_and_payload_where_a_reader_rewrites_them {
    use super::{Row, rows};

    rows! {
        half_quiet: "f97e01" reaches [NanIsAnyNan] misses [];
        half_signalling: "f97c01" reaches [NanIsAnyNan] misses [];
        single_signalling: "fa7f800001" reaches [NanIsAnyNan] misses [];
        double_signalling: "fb7ff0000000000001" reaches [NanIsAnyNan] misses [];
        half_negative: "f9fe00" reaches [NanIsAnyNan] misses [];
        not_a_nan: "f93e00" reaches [] misses [NanIsAnyNan];
    }
}

mod indefinite_lengths_flatten_for_ciborium_and_cbor2 {
    use super::{Row, rows};

    rows! {
        array: "9f0102ff" reaches [CiboriumFlattens, Cbor2Flattens] misses [];
        map: "bf616101ff" reaches [CiboriumFlattens, Cbor2Flattens] misses [];
        byte_chunks: "5f4101420203ff" reaches [CiboriumFlattens, Cbor2Flattens] misses [];
        text_chunks: "7f6161ff" reaches [CiboriumFlattens, Cbor2Flattens] misses [];
        no_byte_chunks: "5fff" reaches [CiboriumFlattens, Cbor2Flattens] misses [];
        empty_array: "9fff" reaches [CiboriumFlattens, Cbor2Flattens] misses [];
        definite: "820102" reaches [] misses [CiboriumFlattens, Cbor2Flattens];
    }
}

mod cbor2_collapses_keys_python_finds_equal {
    use super::{Row, rows};

    rows! {
        int_and_true: "a20100f500" reaches [Cbor2CollapsesKeys] misses [];
        int_and_float: "a20100f93c0001" reaches [Cbor2CollapsesKeys] misses [];
        both_zeros: "a2f9000000f9800001" reaches [Cbor2CollapsesKeys] misses [];
        simple_and_int: "a2e00000f4" reaches [Cbor2CollapsesKeys] misses [];
        arrays_by_position: "a281010081f93c0001" reaches [Cbor2CollapsesKeys] misses [];
        maps_by_key: "a2a20102030400a20304010201" reaches [Cbor2CollapsesKeys] misses [];
        tags_by_content: "a2c50100c5f93c0001" reaches [Cbor2CollapsesKeys] misses [];
        nans: "a2f97e0000f97e0001" reaches [] misses [Cbor2CollapsesKeys];
        simple_and_float: "a2e000f9000001" reaches [] misses [Cbor2CollapsesKeys];
        null_and_undefined: "a2f600f700" reaches [] misses [Cbor2CollapsesKeys];
        bytes_and_text: "a2410000616100" reaches [] misses [Cbor2CollapsesKeys];
        tags_by_number: "a2c50100c60101" reaches [] misses [Cbor2CollapsesKeys];
    }
}

#[test]
fn a_collapsed_key_keeps_its_first_position_and_its_last_value() {
    assert_eq!(
        collapsed(ours("a301000203f50a")),
        map(vec![(int(1), int(10)), (int(2), int(3))])
    );
}

#[test]
fn python_finds_an_int_equal_to_a_bool_and_to_an_integral_float() {
    assert!(equal(&int(1), &Data::Bool(true)));
    assert!(equal(&int(1), &float(1.0)));
    assert!(equal(&float(1.0), &Data::Bool(true)));
    assert!(!equal(&int(2), &Data::Bool(true)));
    assert!(!equal(&int(1), &float(1.5)));
    assert!(!equal(
        &int(9_007_199_254_740_993),
        &float(9_007_199_254_740_992.0)
    ));
}

#[test]
fn python_finds_both_zeros_equal() {
    assert!(equal(&float(-0.0), &float(0.0)));
    assert!(equal(&float(-0.0), &int(0)));
    assert!(!equal(&float(-0.0), &float(f64::MIN_POSITIVE)));
}

#[test]
fn python_finds_no_nan_equal() {
    assert!(!equal(&float(f64::NAN), &float(f64::NAN)));
    assert!(!equal(
        &Data::Float(Float::AnyNan),
        &Data::Float(Float::AnyNan)
    ));
    assert!(equal(&float(f64::INFINITY), &float(f64::INFINITY)));
}

#[test]
fn python_finds_a_simple_value_equal_to_an_int_but_not_a_float() {
    assert!(equal(&Data::Simple(0), &int(0)));
    assert!(equal(&Data::Simple(0), &Data::Bool(false)));
    assert!(equal(&Data::Simple(7), &Data::Simple(7)));
    assert!(!equal(&Data::Simple(0), &float(0.0)));
    assert!(!equal(&Data::Simple(0), &Data::Simple(1)));
}

#[test]
fn python_compares_arrays_by_position_and_maps_by_key() {
    assert!(equal(&array(vec![int(1)]), &array(vec![float(1.0)])));
    assert!(!equal(&array(vec![int(1)]), &array(vec![int(2)])));
    assert!(!equal(
        &array(vec![int(1), int(2)]),
        &array(vec![int(2), int(1)])
    ));
    assert!(equal(
        &map(vec![(int(1), int(2)), (int(3), int(4))]),
        &map(vec![(int(3), int(4)), (int(1), int(2))])
    ));
    assert!(!equal(
        &map(vec![(int(1), int(2))]),
        &map(vec![(int(1), int(3))])
    ));
    assert!(!equal(
        &map(vec![(int(1), int(2))]),
        &map(vec![(int(1), int(2)), (int(3), int(4))])
    ));
}

#[test]
fn python_compares_tags_by_number_and_content() {
    let tag = |number, content| Data::Tag(number, Box::new(content));
    assert!(equal(&tag(5, int(1)), &tag(5, float(1.0))));
    assert!(!equal(&tag(5, int(1)), &tag(6, int(1))));
    assert!(!equal(&tag(5, int(1)), &int(1)));
}

#[test]
fn python_tells_bytes_from_text_and_null_from_undefined() {
    assert!(!equal(&Data::Bytes(b"a".to_vec()), &Data::Text("a".into())));
    assert!(!equal(&Data::Null, &Data::Undefined));
    assert!(equal(&Data::Null, &Data::Null));
}

#[test]
fn any_nan_equals_every_nan_and_nothing_else() {
    assert_eq!(Float::AnyNan, Float::read(f64::NAN));
    assert_eq!(Float::AnyNan, Float::Bits(0x7ff0_0000_0000_0001));
    assert_eq!(Float::AnyNan, Float::Bits(0xfff8_0000_0000_0000));
    assert_ne!(Float::AnyNan, Float::read(f64::INFINITY));
    assert_ne!(Float::AnyNan, Float::read(0.0));
}

#[test]
fn floats_with_bits_compare_by_bits() {
    assert_ne!(Float::read(0.0), Float::read(-0.0));
    assert_ne!(
        Float::Bits(0x7ff8_0000_0000_0000),
        Float::Bits(0x7ff8_0000_0000_0001)
    );
}

#[test]
fn flattening_joins_chunks_and_drops_indefinite_lengths() {
    assert_eq!(
        Data::Array(
            vec![
                Data::BytesChunks(vec![vec![1], vec![2, 3]]),
                Data::TextChunks(vec!["a".into(), "b".into()]),
            ],
            Length::Indefinite
        )
        .flattened(),
        array(vec![Data::Bytes(vec![1, 2, 3]), Data::Text("ab".into())])
    );
}
