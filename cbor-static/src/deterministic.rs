#![forbid(unsafe_code)]
//! Deterministic encoding (D-11b, R-125): `#[cbor(deterministic)]` writes RFC 8949 §4.2.1's order,
//! bytewise by the keys' encodings, and `#[cbor(length_first)]` §4.2.3's, shorter encodings first.
//! A type takes one or neither. This module holds no items.
//!
//! A type's own keys are sorted at compile time:
//!
//! ```
//! #[derive(cbor_static::Cbor)]
//! #[cbor(deterministic)]
//! struct Bytewise {
//!     #[cbor(key = "z")]
//!     z: u8,
//!     #[cbor(key = -1)]
//!     minus_one: u8,
//!     #[cbor(key = 100)]
//!     hundred: u8,
//! }
//!
//! #[derive(cbor_static::Cbor)]
//! #[cbor(length_first)]
//! struct LengthFirst {
//!     #[cbor(key = "z")]
//!     z: u8,
//!     #[cbor(key = -1)]
//!     minus_one: u8,
//!     #[cbor(key = 100)]
//!     hundred: u8,
//! }
//!
//! let mut out = [0u8; 16];
//! let len = cbor_static::encode_into(&Bytewise { z: 1, minus_one: 2, hundred: 3 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa3\x18\x64\x03\x20\x02\x61z\x01");
//! let len = cbor_static::encode_into(&LengthFirst { z: 1, minus_one: 2, hundred: 3 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa3\x20\x02\x18\x64\x03\x61z\x01");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! The type of the value encoded, the root, selects what only the encoder can do: every NaN is
//! written as `f9 7e00`, `-0.0` stays `f9 8000`, and a heapless `LinearMap`'s entries are sorted as
//! they are written. So under a root without either attribute, a deterministic part keeps its
//! sorted keys, but writes its NaNs as they are:
//!
//! ```
//! #[derive(cbor_static::Encode)]
//! #[cbor(deterministic)]
//! struct Part {
//!     zero: f64,
//!     nan: f32,
//! }
//!
//! #[derive(cbor_static::Encode)]
//! struct Root {
//!     part: Part,
//! }
//!
//! let part = Part { zero: -0.0, nan: f32::from_bits(0x7fc0_0001) };
//! let mut out = [0u8; 32];
//! let len = cbor_static::encode_into(&part, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x63nan\xf9\x7e\x00\x64zero\xf9\x80\x00");
//! let len = cbor_static::encode_into(&Root { part }, &mut out)?;
//! assert_eq!(
//!     &out[..len],
//!     b"\xa1\x64part\xa2\x63nan\xfa\x7f\xc0\x00\x01\x64zero\xf9\x80\x00"
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! A flattened struct's keys sort among its parent's, since they share one map (#14 Q34):
//!
//! ```
//! #[derive(cbor_static::Encode)]
//! #[cbor(deterministic)]
//! struct Inner {
//!     c: u8,
//!     a: u8,
//! }
//!
//! #[derive(cbor_static::Encode)]
//! #[cbor(deterministic)]
//! struct Outer {
//!     d: u8,
//!     #[serde(flatten)]
//!     inner: Inner,
//!     b: u8,
//! }
//!
//! let mut out = [0u8; 16];
//! let len = cbor_static::encode_into(&Outer { d: 4, inner: Inner { c: 3, a: 1 }, b: 2 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa4\x61a\x01\x61b\x02\x61c\x03\x61d\x04");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! Every map under a deterministic type is sorted the same way, so each part must write no map of
//! its own or take the same attribute, a flattened struct included. A struct writes a map, as does an enum with a struct
//! variant; a container, such as `Option`, an array or a tuple, writes the maps of what it holds.
//! Anything else does not compile:
//!
//! ```compile_fail,E0080
//! #[derive(cbor_static::Encode)]
//! struct Declared {
//!     a: u8,
//! }
//!
//! #[derive(cbor_static::Encode)]
//! #[cbor(deterministic)]
//! struct Root {
//!     part: Option<Declared>,
//! }
//!
//! let _ = cbor_static::encode_into(&Root { part: None }, &mut [0u8; 8]);
//! ```
//!
//! `RawCbor`, `ArrayRef` and `MapRef` write the bytes they were decoded from (D-11a), sorted or
//! not; decoded under `Strict`, they were sorted:
//!
//! ```
//! #[derive(cbor_static::Cbor)]
//! #[cbor(deterministic)]
//! struct Envelope<'a> {
//!     body: cbor_static::RawCbor<'a>,
//! }
//!
//! let unsorted = b"\xa1\x64body\xa2\x61b\x01\x61a\x02";
//! let mut slot = core::mem::MaybeUninit::uninit();
//! let envelope: &mut Envelope = cbor_static::decode_into(unsorted, &mut slot)?;
//! let mut out = [0u8; 16];
//! let len = cbor_static::encode_into(&*envelope, &mut out)?;
//! assert_eq!(&out[..len], unsorted);
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     cbor_static::decode_strict_into::<Envelope>(unsorted, &mut slot).map(|_| ()).map_err(|e| e.kind),
//!     Err(cbor_static::ErrorKind::NotDeterministic)
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! An internally or adjacently tagged enum writes its tag entry apart from its payload's entries,
//! so it takes neither attribute:
//!
//! ```compile_fail
//! #[derive(cbor_static::Encode)]
//! #[cbor(deterministic)]
//! #[serde(tag = "type")]
//! enum Event {
//!     Start { at: u32 },
//! }
//! ```
//!
//! Nor does a type take both:
//!
//! ```compile_fail
//! #[derive(cbor_static::Encode)]
//! #[cbor(deterministic, length_first)]
//! struct Both {
//!     a: u8,
//! }
//! ```
