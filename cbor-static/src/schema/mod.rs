#![deny(unsafe_code)]
//! Constant descriptors the derives emit. Private: free to change before 1.0.

mod kinds;

use crate::engine::{DECODE_OPTION, ENCODE_OPTION, absent_option, drop_fn, none, some, some_ptr};
use crate::lead::Lead;

#[cfg(feature = "heapless")]
pub use kinds::MapGlue;
pub use kinds::{DecKind, DecVariant, EncKind};

/// Encodes through the engine via a constant descriptor; implemented by `#[derive(Encode)]`.
///
/// # Safety
/// The engine trusts the descriptor without checking it against `Self`. `ENCODE` must describe
/// `Self` exactly: every offset is that of a field of `Self`, every kind is the field type's own
/// `Encode::ENCODE` (or, for a `#[cbor(bytes)]` field, its sealed [`ByteString::ENCODE_BYTES`]),
/// and every handler reads only a valid `Self` or field of it. `Stack` must hold `MAX_DEPTH`
/// frames, and `MAX_DEPTH` must be the deepest nesting `ENCODE` reaches.
/// `OPTION_ENCODE` must describe `Option<Self>` within `Stack`.
#[allow(unsafe_code)]
pub unsafe trait Encode: Sized {
    #[doc(hidden)]
    const ENCODE: &'static EncKind;
    #[doc(hidden)]
    type Stack;
    #[doc(hidden)]
    const MAX_DEPTH: usize;
    /// `Option<Self>`: null, or `Self` in place.
    #[doc(hidden)]
    const OPTION_ENCODE: &'static EncKind = &EncKind::Option {
        ops: &ENCODE_OPTION,
        absent: Some(absent_option::<Self>),
        some: Self::ENCODE,
        some_ptr: some_ptr::<Self>,
    };
}

/// Decodes through the engine via a constant descriptor; implemented by `#[derive(Decode)]`.
///
/// # Safety
/// The engine trusts the descriptor without checking it against `Self`. `DECODE` must describe
/// `Self` exactly: every offset is that of a field of `Self`, every kind is the field type's own
/// `Decode::DECODE` (or, for a `#[cbor(bytes)]` field, its sealed [`ByteString::DECODE_BYTES`]),
/// every handler writes only a valid `Self` or field of it, and the drop handler drops exactly what
/// was written. `Self` borrows the input only through lifetimes `'de`
/// outlives. `Stack` must hold `MAX_DEPTH` frames, and `MAX_DEPTH` must be the deepest nesting
/// `DECODE` reaches. `OPTION_DECODE` must describe `Option<Self>`, and `OptionStack` must hold the
/// frames it reaches.
#[allow(unsafe_code)]
pub unsafe trait Decode<'de>: Sized {
    #[doc(hidden)]
    const DECODE: &'static DecKind;
    #[doc(hidden)]
    type Stack;
    #[doc(hidden)]
    const MAX_DEPTH: usize;
    /// `Option<Self>`: null, or `Self` decoded in place and moved into `Some`, one frame deeper.
    #[doc(hidden)]
    const OPTION_DECODE: &'static DecKind = &DecKind::Option {
        ops: &DECODE_OPTION,
        default: Some(none::<Self>),
        drop: drop_fn::<Option<Self>>(),
        some: Self::DECODE,
        assemble: some::<Self>,
    };
    #[doc(hidden)]
    type OptionStack;
    /// The initial-byte classes `Self` can start with; an untagged enum's variants share none
    /// (D-10).
    #[doc(hidden)]
    const LEAD: Lead = Lead::ANY;
}

/// A byte array or vector's byte-string encoding, which a field selects with `#[cbor(bytes)]`;
/// without it, these are arrays of integers, as in serde (#12 Q21). Neither kind opens a frame.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Key {
///     #[cbor(bytes)]
///     id: [u8; 2],
///     raw: [u8; 2],
/// }
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::encode_into(&Key { id: [1, 2], raw: [1, 2] }, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x62id\x42\x01\x02\x63raw\x82\x01\x02");
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// Any other type does not compile:
///
/// ```compile_fail,E0277
/// #[derive(cbor_static::Cbor)]
/// struct Key {
///     #[cbor(bytes)]
///     id: [u16; 2],
/// }
/// ```
///
/// The engine trusts these kinds, so the trait is sealed: only cbor-static implements it.
///
/// ```compile_fail,E0277
/// struct Mine([u8; 2]);
///
/// impl cbor_static::ByteString for Mine {
///     const ENCODE_BYTES: &'static cbor_static::__private::EncKind =
///         <[u8; 2] as cbor_static::ByteString>::ENCODE_BYTES;
///     const DECODE_BYTES: &'static cbor_static::__private::DecKind =
///         <[u8; 2] as cbor_static::ByteString>::DECODE_BYTES;
/// }
/// ```
pub trait ByteString: sealed::ByteString {
    #[doc(hidden)]
    const ENCODE_BYTES: &'static EncKind;
    #[doc(hidden)]
    const DECODE_BYTES: &'static DecKind;
}

/// A struct encoded as a map, which is what an internally tagged enum's newtype variant must wrap:
/// the tag entry joins the payload's own entries (D-10). The derive implements it for named structs
/// that are not `#[cbor(array)]`.
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// struct Size {
///     w: u32,
/// }
///
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// #[serde(tag = "type")]
/// enum Event {
///     Resize(Size),
/// }
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::encode_into(&Event::Resize(Size { w: 3 }), &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x64type\x66Resize\x61w\x03");
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// Anything else does not compile:
///
/// ```compile_fail,E0277
/// #[derive(cbor_static::Cbor)]
/// #[serde(tag = "type")]
/// enum Event {
///     Count(u32),
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a struct encoded as a map",
    label = "an internally tagged enum's newtype variant must wrap a map-form struct"
)]
pub trait MapForm {}

/// Compiles only for a [`MapForm`] type.
pub const fn map_form<T: MapForm>() {}

/// A field's `default = "path"`: the derive implements it on a type of its own per field, whose
/// `make` calls the path.
pub trait MakeDefault {
    type Value;
    fn make() -> Self::Value;
}

/// The types whose byte-string kinds cbor-static describes, so the engine may trust them.
pub(crate) mod sealed {
    pub trait ByteString {}
}

/// Field or variant names (pre-encoded text items, concatenated) and per-entry placement.
pub struct Layout {
    pub names: &'static [u8],
    pub fields: &'static [FieldLayout],
}

#[derive(Clone, Copy)]
pub struct FieldLayout {
    pub offset: u32,
    pub name_at: u16,
    pub name_len: u8,
    pub omit_if_absent: bool,
}

/// Safe per-enum glue for enums without a primitive `repr`.
pub trait EnumGlue {
    fn variant_index(&self) -> u8;
    fn field_ptr(&self, field: u8) -> *const u8;
}

/// Builds variant `V` from its decoded shadow.
pub trait Assemble<const V: u8>: Sized {
    type Shadow;
    fn assemble(shadow: Self::Shadow) -> Self;
}

impl Layout {
    pub(crate) fn name_item(&self, index: u8) -> &'static [u8] {
        self.fields.get(usize::from(index)).map_or(&[], |field| {
            let start = usize::from(field.name_at) - if field.name_len < 24 { 1 } else { 2 };
            self.names
                .get(start..usize::from(field.name_at) + usize::from(field.name_len))
                .unwrap_or(&[])
        })
    }
}
