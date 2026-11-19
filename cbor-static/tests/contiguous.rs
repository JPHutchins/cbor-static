#![forbid(unsafe_code)]
//! `Decode::CONTIGUOUS` (#5 Q5): borrowed types, internally and adjacently tagged enums, and
//! anything holding one need contiguous input; everything else decodes chunk by chunk.

use cbor_static::{Cbor, Decode, Skipped, Tagged};

fn contiguous<'de, T: Decode<'de>>() -> bool {
    T::CONTIGUOUS
}

#[derive(Cbor)]
struct Owned {
    id: u32,
    at: (i64, Option<[u8; 2]>),
    #[cbor(bytes)]
    key: [u8; 4],
}

#[derive(Cbor)]
struct Borrowed<'a> {
    id: u32,
    name: &'a str,
}

#[derive(Cbor)]
#[serde(tag = "type")]
enum Internal {
    Start,
}

#[derive(Cbor)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Data(u32),
}

#[derive(Cbor)]
enum External<'a> {
    Count(u32),
    Name { name: &'a str },
}

#[derive(Cbor)]
enum Plain {
    Count(u32),
    Pair { a: u8, b: Option<u8> },
}

#[derive(Cbor)]
#[serde(untagged)]
enum Untagged<'a> {
    Count(u64),
    Name(&'a str),
}

#[derive(Cbor)]
#[serde(untagged)]
enum OwnedUntagged {
    Count(u64),
    Flag(bool),
}

#[derive(Cbor)]
#[cbor(index)]
enum Index<'a> {
    Ping,
    Name(&'a str),
}

#[derive(Cbor)]
#[cbor(index)]
enum OwnedIndex {
    Ping,
    Set(u32),
}

#[derive(Cbor)]
enum Envelope<'a> {
    #[cbor(cbor_tag = 18)]
    Signed(&'a [u8]),
    #[cbor(cbor_tag = 17)]
    Maced(u32),
}

#[derive(Cbor)]
enum OwnedEnvelope {
    #[cbor(cbor_tag = 18)]
    Signed(u32),
}

#[derive(Cbor)]
enum Frame {
    Raw(#[cbor(bytes)] [u8; 4]),
}

#[derive(Cbor)]
struct Limited<'a> {
    #[cbor(max_len = 4)]
    name: &'a str,
}

#[derive(Cbor)]
struct Holds<T> {
    inner: Option<T>,
}

#[derive(Cbor)]
struct Wrapper<'a>(&'a [u8]);

#[derive(Cbor)]
struct Unit;

#[derive(Cbor)]
struct Flattening<'a> {
    id: u32,
    #[serde(flatten)]
    inner: Borrowed<'a>,
}

#[derive(Cbor)]
struct Skipping<'a> {
    id: u32,
    #[serde(skip)]
    name: &'a str,
}

#[derive(Cbor)]
struct Counting {
    id: u32,
    #[cbor(other)]
    rest: Skipped,
}

#[test]
fn owned_types_decode_in_chunks() {
    assert!(!contiguous::<u64>());
    assert!(!contiguous::<u128>());
    assert!(!contiguous::<char>());
    assert!(!contiguous::<Option<[(u8, bool); 3]>>());
    assert!(!contiguous::<Tagged<1, f64>>());
    assert!(!contiguous::<Owned>());
    assert!(!contiguous::<Plain>());
    assert!(!contiguous::<Holds<u32>>());
    assert!(!contiguous::<Unit>());
    assert!(!contiguous::<OwnedUntagged>());
    assert!(!contiguous::<OwnedIndex>());
    assert!(!contiguous::<OwnedEnvelope>());
    assert!(!contiguous::<Frame>());
}

#[test]
fn every_enum_form_but_internal_and_adjacent_takes_its_variants() {
    assert!(contiguous::<Untagged<'_>>());
    assert!(contiguous::<Index<'_>>());
    assert!(contiguous::<Envelope<'_>>());
}

#[test]
fn a_length_limit_keeps_its_field_borrowed() {
    assert!(contiguous::<Limited<'_>>());
}

#[test]
fn borrowed_types_need_contiguous_input() {
    use cbor_static::{ArrayRef, Chunks, MapRef, RawCbor};

    assert!(contiguous::<&str>());
    assert!(contiguous::<&[u8]>());
    assert!(contiguous::<RawCbor<'_>>());
    assert!(contiguous::<ArrayRef<'_, u8>>());
    assert!(contiguous::<MapRef<'_, u8, u8>>());
    assert!(contiguous::<Chunks<'_, str>>());
    assert!(contiguous::<cbor_static::Embedded<u8>>());
    assert!(contiguous::<Borrowed<'_>>());
    assert!(contiguous::<Wrapper<'_>>());
}

#[test]
fn internally_and_adjacently_tagged_enums_need_contiguous_input() {
    assert!(contiguous::<Internal>());
    assert!(contiguous::<Adjacent>());
}

#[test]
fn holding_a_contiguous_type_makes_a_type_contiguous() {
    assert!(contiguous::<Option<&str>>());
    assert!(contiguous::<[Internal; 2]>());
    assert!(contiguous::<(u8, Adjacent)>());
    assert!(contiguous::<Tagged<1, &[u8]>>());
    assert!(contiguous::<Holds<Internal>>());
    assert!(contiguous::<External<'_>>());
    assert!(contiguous::<Flattening<'_>>());
}

#[test]
fn every_forwarder_is_contiguous_exactly_when_a_child_is() {
    use cbor_static::SelfDescribed;

    assert!(!contiguous::<Option<u8>>());
    assert!(contiguous::<Option<&str>>());
    assert!(!contiguous::<[u8; 2]>());
    assert!(contiguous::<[&str; 2]>());
    assert!(!contiguous::<(u8, u8)>());
    assert!(contiguous::<(&str, u8)>());
    assert!(contiguous::<(u8, &str)>());
    assert!(!contiguous::<Tagged<1, u8>>());
    assert!(contiguous::<Tagged<1, &str>>());
    assert!(!contiguous::<SelfDescribed<u8>>());
    assert!(contiguous::<SelfDescribed<&str>>());
}

#[test]
fn skipped_fields_and_counted_entries_add_nothing() {
    assert!(!contiguous::<Skipping<'_>>());
    assert!(!contiguous::<Counting>());
}

#[cfg(feature = "heapless")]
mod heapless_types {
    use cbor_static::{Cbor, RawEntry};
    use heapless::{LinearMap, String, Vec};

    use super::{Borrowed, Internal, contiguous};

    #[derive(Cbor)]
    struct Capturing<'a> {
        id: u32,
        #[cbor(other)]
        rest: Vec<RawEntry<'a>, 2>,
    }

    #[derive(Cbor)]
    struct Bytes {
        #[cbor(bytes)]
        key: Vec<u8, 4>,
    }

    #[test]
    fn heapless_containers_take_their_elements() {
        assert!(!contiguous::<String<4>>());
        assert!(!contiguous::<Vec<u32, 4>>());
        assert!(!contiguous::<LinearMap<u8, u32, 4>>());
        assert!(!contiguous::<Bytes>());
        assert!(contiguous::<Vec<Borrowed<'_>, 4>>());
        assert!(contiguous::<LinearMap<u8, Internal, 4>>());
        assert!(contiguous::<LinearMap<&str, u8, 4>>());
        assert!(!contiguous::<Vec<u8, 4>>());
        assert!(contiguous::<Vec<&str, 4>>());
        assert!(contiguous::<LinearMap<u8, &str, 4>>());
    }

    #[test]
    fn captured_entries_borrow_the_input() {
        assert!(contiguous::<Capturing<'_>>());
    }
}

#[cfg(feature = "alloc")]
mod alloc_types {
    use std::collections::BTreeMap;

    use cbor_static::Bounded;

    use super::contiguous;

    #[test]
    fn alloc_containers_take_their_elements() {
        assert!(!contiguous::<String>());
        assert!(!contiguous::<Vec<u8>>());
        assert!(contiguous::<Vec<&str>>());
        assert!(!contiguous::<Box<u8>>());
        assert!(contiguous::<Box<&str>>());
        assert!(!contiguous::<BTreeMap<u8, u8>>());
        assert!(contiguous::<BTreeMap<&str, u8>>());
        assert!(contiguous::<BTreeMap<u8, &str>>());
        assert!(!contiguous::<Bounded<Vec<u8>, 2>>());
        assert!(contiguous::<Bounded<Vec<&str>, 2>>());
    }
}
