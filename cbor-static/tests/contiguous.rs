#![forbid(unsafe_code)]
//! `Decode::CONTIGUOUS` (#5 Q5): borrowed types, internally and adjacently tagged enums, and
//! anything holding one need contiguous input; everything else decodes chunk by chunk.

use cbor_static::{ArrayRef, Cbor, Chunks, Decode, MapRef, RawCbor, Skipped, Tagged};

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
}

#[test]
fn borrowed_types_need_contiguous_input() {
    assert!(contiguous::<&str>());
    assert!(contiguous::<&[u8]>());
    assert!(contiguous::<RawCbor<'_>>());
    assert!(contiguous::<ArrayRef<'_, u8>>());
    assert!(contiguous::<MapRef<'_, u8, u8>>());
    assert!(contiguous::<Chunks<'_, str>>());
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
    }

    #[test]
    fn captured_entries_borrow_the_input() {
        assert!(contiguous::<Capturing<'_>>());
    }
}
