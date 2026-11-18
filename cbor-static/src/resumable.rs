#![deny(unsafe_code)]
//! What decoding from a reader asks of a type (#5 Q5): that it owns what it decodes, for any input
//! lifetime, and that its decoding never needs the whole item in one chunk (D-16, D-19).

use crate::schema::Decode;

/// Compiles only for a type every input chunk can decode into.
///
/// ```
/// #[derive(cbor_static::Decode)]
/// struct Reading {
///     id: u32,
///     #[cbor(bytes)]
///     key: [u8; 4],
/// }
///
/// cbor_static::__private::resumable::<Reading>();
/// ```
///
/// An internally tagged enum needs contiguous input:
///
/// ```compile_fail,E0080
/// #[derive(cbor_static::Decode)]
/// #[serde(tag = "type")]
/// enum Event {
///     Start,
/// }
///
/// cbor_static::__private::resumable::<Event>();
/// ```
///
/// A type that borrows the input does too:
///
/// ```compile_fail
/// #[derive(cbor_static::Decode)]
/// struct Reading<'a> {
///     name: &'a str,
/// }
///
/// cbor_static::__private::resumable::<Reading<'static>>();
/// ```
pub const fn resumable<T: for<'de> Decode<'de>>() {
    const {
        assert!(
            !<T as Decode<'static>>::CONTIGUOUS,
            "cbor-static: the type's decoding needs contiguous input"
        );
    };
}
