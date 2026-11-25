//! Python's equality on the values cbor2 builds, and the dict it builds from a map: keys that
//! Python finds equal collapse into the first, which takes the last one's value.

use super::{Data, Float};

/// Python's `==` on what cbor2 decodes each item to, for items without chunks.
///
/// Integers, booleans and simple values are Python ints (`CBORSimpleValue` equals an int, but
/// not a float); a float equals an int of the same value; NaN equals nothing; arrays become tuples
/// and maps become dicts, which compare without regard to order.
#[expect(clippy::float_cmp, reason = "Python's float equality is exact")]
pub fn equal(left: &Data, right: &Data) -> bool {
    match (number(left), number(right)) {
        (Some(Number::Int { value: x, .. }), Some(Number::Int { value: y, .. })) => x == y,
        (Some(Number::Float(x)), Some(Number::Float(y))) => x == y,
        (Some(Number::Int { value, simple }), Some(Number::Float(float)))
        | (Some(Number::Float(float)), Some(Number::Int { value, simple })) => {
            !simple && float.fract() == 0.0 && exact(float) == value
        }
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => match (left, right) {
            (Data::Bytes(x), Data::Bytes(y)) => x == y,
            (Data::Text(x), Data::Text(y)) => x == y,
            (Data::Array(x, _), Data::Array(y, _)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(x, y)| equal(x, y))
            }
            (Data::Map(x, _), Data::Map(y, _)) => {
                x.len() == y.len()
                    && x.iter().all(|(key, value)| {
                        y.iter()
                            .any(|(other, found)| equal(key, other) && equal(value, found))
                    })
            }
            (Data::Tag(tag, content), Data::Tag(other, found)) => {
                tag == other && equal(content, found)
            }
            (Data::Null, Data::Null) | (Data::Undefined, Data::Undefined) => true,
            _ => false,
        },
    }
}

/// Every map as the dict cbor2 builds from it, at any depth.
pub fn collapsed(data: Data) -> Data {
    match data {
        Data::Map(entries, length) => Data::Map(
            entries
                .into_iter()
                .map(|(key, value)| (collapsed(key), collapsed(value)))
                .fold(
                    Vec::new(),
                    |dict: Vec<(Data, Data)>, (key, value)| match dict
                        .iter()
                        .position(|(seen, _)| equal(seen, &key))
                    {
                        Some(at) => dict
                            .into_iter()
                            .enumerate()
                            .map(|(index, (seen, found))| {
                                (seen, if index == at { value.clone() } else { found })
                            })
                            .collect(),
                        None => dict.into_iter().chain([(key, value)]).collect(),
                    },
                ),
            length,
        ),
        Data::Array(items, length) => {
            Data::Array(items.into_iter().map(collapsed).collect(), length)
        }
        Data::Tag(tag, content) => Data::Tag(tag, Box::new(collapsed(*content))),
        leaf @ (Data::Integer(_)
        | Data::Bytes(_)
        | Data::Text(_)
        | Data::BytesChunks(_)
        | Data::TextChunks(_)
        | Data::Simple(_)
        | Data::Bool(_)
        | Data::Null
        | Data::Undefined
        | Data::Float(_)) => leaf,
    }
}

#[derive(Clone, Copy)]
enum Number {
    Int { value: i128, simple: bool },
    Float(f64),
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "an integral float within an item's range converts exactly"
)]
fn exact(float: f64) -> i128 {
    float as i128
}

fn number(data: &Data) -> Option<Number> {
    match *data {
        Data::Integer(value) => Some(Number::Int {
            value,
            simple: false,
        }),
        Data::Bool(value) => Some(Number::Int {
            value: value.into(),
            simple: false,
        }),
        Data::Simple(value) => Some(Number::Int {
            value: value.into(),
            simple: true,
        }),
        Data::Float(Float::Bits(bits)) => Some(Number::Float(f64::from_bits(bits))),
        Data::Float(Float::AnyNan) => Some(Number::Float(f64::NAN)),
        Data::Bytes(_)
        | Data::Text(_)
        | Data::BytesChunks(_)
        | Data::TextChunks(_)
        | Data::Array(..)
        | Data::Map(..)
        | Data::Tag(..)
        | Data::Null
        | Data::Undefined => None,
    }
}
