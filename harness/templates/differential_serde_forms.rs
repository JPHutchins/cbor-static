use std::collections::BTreeMap;

use serde::Serialize;

mod common;

#[allow(dead_code)]
#[path = "$common"]
mod oracles;

use common::{ciborium_bytes, serde_layer};

#[derive(Serialize)]
struct Unit;

#[derive(Serialize)]
struct Newtype(u32);

#[derive(Serialize)]
struct Tuple(u32, bool);

#[derive(Serialize)]
struct Fields {
    a: u32,
    b: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    c: Option<u32>,
}

#[derive(Serialize)]
enum External {
    Unit,
    Newtype(u32),
    Tuple(u32, bool),
    Struct { x: u32 },
}

#[derive(Serialize)]
#[serde(tag = "t")]
enum Internal {
    Unit,
    Struct { x: u32 },
    Newtype(Fields),
}

#[derive(Serialize)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Unit,
    Newtype(u32),
    Struct { x: u32 },
}

#[derive(Serialize)]
#[serde(untagged)]
enum Untagged {
    Int(u32),
    Text(&'static str),
    Pair(u32, u32),
}

/// A sequence serde cannot count ahead: its iterator gives no exact size, so `collect_seq` calls
/// `serialize_seq(None)`.
struct UnknownLength(Vec<u32>);

impl Serialize for UnknownLength {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().filter(|_| true))
    }
}

#[derive(Serialize)]
struct Flattened {
    a: u32,
    #[serde(flatten)]
    rest: Fields,
}

/// The serde layer and ciborium write the same bytes.
fn same<T: Serialize>(value: &T) {
    assert_eq!(ciborium_bytes(value), serde_layer(value));
}

/// A form ciborium writes with an unknown length: other bytes than the serde layer's, but the
/// same data, as `Tokens` reads both.
fn same_data<T: Serialize>(value: &T) {
    let data = |bytes: &[u8]| oracles::readers::tokens(bytes).map(oracles::Data::flattened);
    let (theirs, ours) = (ciborium_bytes(value), serde_layer(value));
    assert_ne!(theirs, ours);
    assert_eq!(data(&theirs), data(&ours));
}

#[test]
fn unit_struct() {
    same(&Unit);
}

#[test]
fn newtype_struct() {
    same(&Newtype(24));
}

#[test]
fn tuple_struct() {
    same(&Tuple(256, true));
}

#[test]
fn fields_with_and_without_options() {
    same(&Fields { a: 1, b: None, c: None });
    same(&Fields { a: 1, b: Some(2), c: Some(3) });
}

#[test]
fn externally_tagged_variants() {
    same(&External::Unit);
    same(&External::Newtype(65_536));
    same(&External::Tuple(1, false));
    same(&External::Struct { x: 23 });
}

#[test]
fn internally_tagged_variants() {
    same(&Internal::Unit);
    same(&Internal::Struct { x: 2 });
    same(&Internal::Newtype(Fields { a: 1, b: None, c: Some(3) }));
}

#[test]
fn adjacently_tagged_variants() {
    same(&Adjacent::Unit);
    same(&Adjacent::Newtype(7));
    same(&Adjacent::Struct { x: 8 });
}

#[test]
fn untagged_variants() {
    same(&Untagged::Int(4_294_967_295));
    same(&Untagged::Text("text"));
    same(&Untagged::Pair(1, 2));
}

#[test]
fn flattened_fields_compare_as_data() {
    same_data(&Flattened { a: 1, rest: Fields { a: 2, b: Some(3), c: None } });
}

#[test]
fn a_sequence_of_unknown_length_compares_as_data() {
    same_data(&UnknownLength(vec![1, 24, 256]));
}

#[test]
fn options_and_unit() {
    same(&Option::<u32>::None);
    same(&Some(5u32));
    same(&());
    same(&Some(()));
}

#[test]
fn sequences_maps_and_tuples() {
    same(&vec![1u32, 24, 256]);
    same(&BTreeMap::from([("a".to_owned(), 1u32), ("b".to_owned(), 2)]));
    same(&(1u8, true, "x"));
}

#[test]
fn integers_at_each_head_width() {
    same(&[0u64, 23, 24, 255, 256, 65_535, 65_536, 4_294_967_295, 4_294_967_296, u64::MAX]);
    same(&[-1i64, -24, -25, -256, -257, i64::MIN]);
}

#[test]
fn floats_at_each_width() {
    same(&[1.5f32, 0.1f32]);
    same(&[1.5f64, 1.1, -0.0, f64::INFINITY, f64::NAN, f64::from(f32::MAX)]);
}

#[test]
fn text_bytes_and_chars() {
    same(&"text");
    same(&'é');
    same(&serde_bytes::Bytes::new(&[0, 1, 2]));
}
