#![forbid(unsafe_code)]
//! The serde attributes the derives honour beyond names and keys (D-08, #13 Q30 and Q31). Each is
//! written `#[serde(...)]` or `#[cbor(...)]`; any other serde attribute is a compile error. This
//! module holds no items.
//!
//! # `rename_all`
//!
//! `"snake_case"`, `"camelCase"` or `"PascalCase"` renames, as serde does, each field of a struct,
//! or each variant of an enum, that has no `rename` of its own:
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
//! let mut out = [0u8; 32];
//! let len = cbor_static::encode_into(&Header { key_ops: 1, alg_id: -7 }, &mut out)?;
//! assert_eq!(&out[..len], b"\xa2\x66keyOps\x01\x63Alg\x26");
//! let len = cbor_static::encode_into(&Command::ReadAll, &mut out)?;
//! assert_eq!(&out[..len], b"\x68read_all");
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
//! let len = cbor_static::encode_into(&Reading { id: 1, cache: Cache(9) }, &mut out)?;
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
//! let len = cbor_static::encode_into(&Meters { value: 1000 }, &mut out)?;
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
