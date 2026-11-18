#![forbid(unsafe_code)]
//! The serde attributes the derives honour beyond names and keys (D-08, #13 Q30 and Q31, #14
//! Q34). Each is written `#[serde(...)]` or `#[cbor(...)]`; any other serde attribute is a compile
//! error. This module holds no items.
//!
//! # `rename_all`
//!
//! `"snake_case"`, `"camelCase"`, `"PascalCase"` or `"kebab-case"` renames, as serde does, each
//! field of a struct, or each variant of an enum, that has no `rename` of its own:
//!
//! ```
//! #[derive(cbor_static::Cbor)]
//! #[serde(rename_all = "camelCase")]
//! struct Header {
//!     key_ops: u32,
//!     #[serde(rename = "Alg")]
//!     alg_id: i32,
//! }
//!
//! #[derive(cbor_static::Cbor)]
//! #[serde(rename_all = "snake_case")]
//! enum Command {
//!     ReadAll,
//! }
//!
//! #[derive(cbor_static::Cbor)]
//! #[serde(rename_all = "PascalCase")]
//! struct Params {
//!     base_iv: u32,
//! }
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[serde(rename_all = "kebab-case")]
//! enum Mode {
//!     WriteThrough,
//! }
//!
//! let mut out = [0u8; 32];
//! let len = cbor_static::to_slice(&Header { key_ops: 1, alg_id: -7 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x66keyOps\x01\x63Alg\x26");
//! let len = cbor_static::to_slice(&Command::ReadAll, &mut out)?;
//! assert_eq!(&out[..len], b"\x68read_all");
//! let len = cbor_static::to_slice(&Params { base_iv: 2 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa1\x66BaseIv\x02");
//! let len = cbor_static::to_slice(&Mode::WriteThrough, &mut out)?;
//! assert_eq!(&out[..len], b"\x6dwrite-through");
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     *cbor_static::decode_into::<Mode>(&out[..len], &mut slot)?,
//!     Mode::WriteThrough
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! # `default` and `default = "path"`
//!
//! A field its map or array lacks decodes to `T::default()`, or to what the path returns:
//!
//! ```
//! fn five() -> u32 {
//!     5
//! }
//!
//! #[derive(cbor_static::Decode, Debug, PartialEq)]
//! struct Config {
//!     id: u32,
//!     #[serde(default)]
//!     retries: u8,
//!     #[serde(default = "five")]
//!     timeout: u32,
//! }
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     *cbor_static::decode_into::<Config>(b"\xa1\x62id\x01", &mut slot)?,
//!     Config { id: 1, retries: 0, timeout: 5 }
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! A field with `default` needs a type that implements `Default`:
//!
//! ```compile_fail,E0277
//! #[derive(cbor_static::Decode)]
//! struct Port(u16);
//!
//! #[derive(cbor_static::Decode)]
//! struct Config {
//!     #[serde(default)]
//!     port: Port,
//! }
//! ```
//!
//! # `skip`
//!
//! A skipped field is never written, and decodes to its default, or with `default = "path"` to
//! what the path returns. Its type needs no codec trait:
//!
//! ```
//! #[derive(Debug, Default, PartialEq)]
//! struct Cache(u32);
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Reading {
//!     id: u32,
//!     #[serde(skip)]
//!     cache: Cache,
//! }
//!
//! let mut out = [0u8; 8];
//! let len = cbor_static::to_slice(&Reading { id: 1, cache: Cache(9) }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa1\x62id\x01");
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     *cbor_static::decode_into::<Reading>(&out[..len], &mut slot)?,
//!     Reading { id: 1, cache: Cache(0) }
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! # `transparent`
//!
//! A struct with exactly one field is that field on the wire:
//!
//! ```
//! #[derive(cbor_static::Cbor)]
//! #[serde(transparent)]
//! struct Meters {
//!     value: u32,
//! }
//!
//! let mut out = [0u8; 4];
//! let len = cbor_static::to_slice(&Meters { value: 1000 }, &mut out)?;
//! assert_eq!(&out[..len], b"\x19\x03\xe8");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! # `borrow`
//!
//! A field that borrows decodes zero-copy with or without it; it is accepted as serde accepts it,
//! on a field whose type has a lifetime to borrow:
//!
//! ```
//! #[derive(cbor_static::Decode, Debug, PartialEq)]
//! struct Named<'a> {
//!     #[serde(borrow)]
//!     name: &'a str,
//! }
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     *cbor_static::decode_into::<Named>(b"\xa1\x64name\x61a", &mut slot)?,
//!     Named { name: "a" }
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! # `flatten`
//!
//! A field whose type is a derived map-form struct brings that struct's entries into its own
//! struct's map, in its place, at compile time (#14 Q34). Its struct alone decides what an unknown
//! entry does: the flattened struct's own `deny_unknown_fields` does not apply.
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! #[serde(deny_unknown_fields)]
//! struct Labels {
//!     #[cbor(key = 1)]
//!     alg: i8,
//! }
//!
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Header {
//!     id: u8,
//!     #[serde(flatten)]
//!     labels: Labels,
//!     seq: u8,
//! }
//!
//! let header = Header { id: 1, labels: Labels { alg: -7 }, seq: 2 };
//! let mut out = [0u8; 16];
//! let len = cbor_static::to_slice(&header, &mut out)?;
//! assert_eq!(&out[..len], b"\xa3\x62id\x01\x01\x26\x63seq\x02");
//!
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     *cbor_static::decode_into::<Header>(b"\xa4\x63seq\x02\x05\x00\x01\x26\x62id\x01", &mut slot)?,
//!     header
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! The map's keys must differ, its flattened entries' included:
//!
//! ```compile_fail,E0080
//! #[derive(cbor_static::Encode)]
//! struct Labels {
//!     id: u8,
//! }
//!
//! #[derive(cbor_static::Encode)]
//! struct Header {
//!     id: u8,
//!     #[serde(flatten)]
//!     labels: Labels,
//! }
//!
//! let _ = cbor_static::to_slice(&Header { id: 1, labels: Labels { id: 2 } }, &mut [0u8; 16]);
//! ```
//!
//! And the field's type must be a struct encoded as a map:
//!
//! ```compile_fail,E0277
//! #[derive(cbor_static::Encode)]
//! struct Header {
//!     #[serde(flatten)]
//!     count: u32,
//! }
//! ```
//!
//! # `other`
//!
//! `#[cbor(other)]` on one field of a map-form struct, a flattened struct's included, takes the
//! entries no field claims (R-128): [`crate::Skipped`] counts them, and with feature `alloc` or
//! `heapless`, a `Vec` of [`crate::RawEntry`] captures each whole and writes it back in its place. It
//! is a cbor-static attribute; serde's variant-level `other` is unsupported. `Strict` still rejects
//! an unknown entry, and `deny_unknown_fields` beside it is a compile error:
//!
//! ```compile_fail
//! #[derive(cbor_static::Cbor)]
//! #[serde(deny_unknown_fields)]
//! struct Header {
//!     id: u8,
//!     #[cbor(other)]
//!     unknown: cbor_static::Skipped,
//! }
//! ```
//!
//! # `max_len = N`
//!
//! `#[cbor(max_len = N)]` bounds a field's `len()`: bytes for `&str`, `&[u8]` and a `String`,
//! elements for a `Vec`, entries for `BTreeMap` and `LinearMap`, alloc's and heapless's alike, and an `Option` of
//! any of them when it is `Some` (#5 Q15). Encoding a longer value fails with `LimitExceeded`, and so does
//! decoding one: a definite length at its head, before any copy, and an indefinite one at the
//! chunk or element that would pass `N`. A borrowed `&str` or `&[u8]` copies nothing and is
//! checked once its payload is in one chunk, so a truncated one is `UnexpectedEnd`, and one split
//! across chunks `NeedContiguous` (D-16).
//!
//! ```
//! #[derive(cbor_static::Cbor, Debug, PartialEq)]
//! struct Name<'a> {
//!     #[cbor(max_len = 4)]
//!     name: &'a str,
//! }
//!
//! assert_eq!(
//!     cbor_static::to_slice(&Name { name: "probe" }, &mut [0u8; 16]).map_err(|e| e.kind),
//!     Err(cbor_static::ErrorKind::LimitExceeded)
//! );
//! let mut slot = core::mem::MaybeUninit::uninit();
//! assert_eq!(
//!     cbor_static::decode_into::<Name>(b"\xa1\x64name\x65probe", &mut slot).map_err(|e| e.kind),
//!     Err(cbor_static::ErrorKind::LimitExceeded)
//! );
//! assert_eq!(
//!     *cbor_static::decode_into::<Name>(b"\xa1\x64name\x64prob", &mut slot)?,
//!     Name { name: "prob" }
//! );
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! A type whose length is fixed, such as `[T; N]`, or that it does not bound yet, such as
//! [`crate::Chunks`], [`crate::ArrayRef`] and [`crate::MapRef`], takes none:
//!
//! ```compile_fail,E0277
//! #[derive(cbor_static::Cbor)]
//! struct Key {
//!     #[cbor(max_len = 2)]
//!     id: [u8; 4],
//! }
//!
//! let _ = cbor_static::to_slice(&Key { id: [0; 4] }, &mut [0u8; 16]);
//! ```
//!
//! With a limit of its own, such as heapless `String<N>`'s, the smaller one applies, and a
//! `max_len` above it does not compile.
//!
//! # `max_encoded_len`
//!
//! `#[cbor(max_encoded_len)]` on a struct or enum implements [`crate::MaxEncodedLen`], which
//! [`crate::encode_into`] sizes its buffer by (#5 Q2). Every field it writes must be bounded: by
//! its type, by `#[cbor(bytes)]` over a fixed-capacity byte string, or by `max_len`. A generic
//! type's parameters must be bounded too.
//!
//! ```
//! use cbor_static::MaxEncodedLen;
//!
//! #[derive(cbor_static::Encode)]
//! #[cbor(max_encoded_len)]
//! struct Reading<'a> {
//!     id: u16,
//!     #[cbor(max_len = 8)]
//!     name: &'a str,
//! }
//!
//! assert_eq!(Reading::MAX_ENCODED_LEN, 21);
//! let mut out = [0u8; Reading::MAX_ENCODED_LEN];
//! let len = cbor_static::encode_into(&Reading { id: 7, name: "probe" }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x62id\x07\x64name\x65probe");
//! # Ok::<(), cbor_static::Error>(())
//! ```
//!
//! An unbounded field does not compile, and rustc names it:
//!
//! ```compile_fail,E0277
//! #[derive(cbor_static::Encode)]
//! #[cbor(max_encoded_len)]
//! struct Note<'a> {
//!     text: &'a str,
//! }
//! ```
//!
//! The impl goes with `Encode`, so a type deriving `Decode` alone has none:
//!
//! ```compile_fail,E0277
//! #[derive(cbor_static::Decode)]
//! #[cbor(max_encoded_len)]
//! struct Ack {
//!     ok: bool,
//! }
//!
//! let _ = <Ack as cbor_static::MaxEncodedLen>::MAX_ENCODED_LEN;
//! ```
