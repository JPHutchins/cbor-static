#![deny(unsafe_code)]
//! Constant descriptors the derives emit. Private: free to change before 1.0.

mod flat;
mod kinds;
mod order;

use crate::engine::{
    DECODE_OPTION, DeterministicOps, ENCODE_OPTION, absent_option, drop_fn, none, some, some_ptr,
};
use crate::lead::Lead;

pub use flat::{Concat, Flat, Run, Storage, entries, flattened};
#[cfg(feature = "heapless")]
pub use kinds::MapGlue;
pub use kinds::{DecKind, DecVariant, EncKind};
pub use order::Order;

/// Encodes through the engine via a constant descriptor; implemented by `#[derive(Encode)]`, for
/// generic types too, each type parameter bounded by the trait (R-127):
///
/// ```
/// #[derive(cbor_static::Cbor, Debug, PartialEq)]
/// enum Either<L, R> {
///     Left(L),
///     Right { value: R },
/// }
///
/// let mut out = [0u8; 16];
/// let len = cbor_static::encode_into(&Either::<u8, bool>::Right { value: true }, &mut out)?;
/// assert_eq!(&out[..len], b"\xa1\x65Right\xa1\x65value\xf5");
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// assert_eq!(
///     *cbor_static::decode_into::<Either<u8, bool>>(b"\xa1\x64Left\x07", &mut slot)?,
///     Either::Left(7)
/// );
/// # Ok::<(), cbor_static::Error>(())
/// ```
///
/// # Safety
/// The engine trusts the descriptor without checking it against `Self`. `ENCODE` must describe
/// `Self` exactly: every offset is that of a field of `Self`, every kind is the field type's own
/// `Encode::ENCODE` (or, for a `#[cbor(bytes)]` field, its sealed [`ByteString::ENCODE_BYTES`]),
/// and every handler reads only a valid `Self` or field of it. `Stack` must hold `MAX_DEPTH`
/// frames, and `MAX_DEPTH` must be the deepest nesting `ENCODE` reaches.
/// `OPTION_ENCODE` must describe `Option<Self>` within `Stack`.
///
/// A `Stack` that does not hold `MAX_DEPTH` frames does not compile where the type is encoded
/// (D-14, R-111):
///
/// ```compile_fail,E0080
/// struct Liar(u32);
///
/// unsafe impl cbor_static::Encode for Liar {
///     const ENCODE: &'static cbor_static::__private::EncKind = <u32 as cbor_static::Encode>::ENCODE;
///     type Stack = ();
///     const MAX_DEPTH: usize = 1;
/// }
///
/// let _ = cbor_static::encode_into(&Liar(1), &mut [0u8; 8]);
/// ```
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
    /// The deterministic encoding a value of this type selects as the root (D-11b).
    #[doc(hidden)]
    const DETERMINISTIC: Option<&'static DeterministicOps> = None;
    /// The map key order this type's encoding commits to (D-11b).
    #[doc(hidden)]
    const ORDER: Order = Order::Any;
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
///
/// A `Stack` that does not hold `MAX_DEPTH` frames does not compile where the type is decoded
/// (D-14, R-111):
///
/// ```compile_fail,E0080
/// #[derive(Debug)]
/// struct Liar(u32);
///
/// unsafe impl<'de> cbor_static::Decode<'de> for Liar {
///     const DECODE: &'static cbor_static::__private::DecKind =
///         <u32 as cbor_static::Decode<'de>>::DECODE;
///     type Stack = ();
///     const MAX_DEPTH: usize = 2;
///     type OptionStack = ();
/// }
///
/// let mut slot = core::mem::MaybeUninit::uninit();
/// let _ = cbor_static::decode_into::<Liar>(b"\x01", &mut slot);
/// ```
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

/// What a map-form struct's `#[cbor(other)]` field may be (R-128): [`crate::Skipped`], which counts
/// the entries no field claims, or with feature `heapless`, a `heapless::Vec` of
/// [`crate::RawEntry`], which captures them. The engine trusts these kinds, so the trait is sealed.
///
/// ```compile_fail,E0277
/// #[derive(cbor_static::Cbor)]
/// struct Header {
///     #[cbor(other)]
///     unknown: u32,
/// }
/// ```
pub trait Other: sealed::Other {
    #[doc(hidden)]
    const ENCODE_OTHER: &'static EncKind;
    #[doc(hidden)]
    const DECODE_OTHER: &'static DecKind;
    /// Captured entries cannot be sorted, so a deterministic type cannot hold them (D-11b).
    #[doc(hidden)]
    const ORDER: Order;
    #[doc(hidden)]
    type Stack;
    #[doc(hidden)]
    const MAX_DEPTH: usize;
}

/// A struct encoded as a map, which is what an internally tagged enum's newtype variant must wrap:
/// the tag entry joins the payload's own entries (D-10). The derive implements it for named structs
/// that are not `#[cbor(array)]`, in each direction it derives.
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
pub trait MapForm<Direction> {}

/// [`MapForm`]'s directions: each derive implements it for the one it derives, so a type that
/// derives `Encode` and `Decode` separately implements it once per direction.
pub enum Encoding {}
pub enum Decoding {}

/// Compiles only for a [`MapForm`] type in `Direction`.
pub const fn map_form<T: MapForm<Direction>, Direction>() {}

/// A field's `default = "path"`: the derive implements it on a type of its own per field, whose
/// `make` calls the path.
pub trait MakeDefault {
    type Value;
    fn make() -> Self::Value;
}

/// The types whose byte-string kinds cbor-static describes, so the engine may trust them.
pub(crate) mod sealed {
    pub trait ByteString {}
    pub trait Other {}
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
            let start = usize::from(field.name_at) - head_before(field.name_len);
            self.names
                .get(start..usize::from(field.name_at) + usize::from(field.name_len))
                .unwrap_or(&[])
        })
    }
}

/// The bytes of an identifier's item before its `name_at`, by its `name_len`: its initial byte, and
/// the length byte of a text of 24 bytes or more.
pub(crate) const fn head_before(name_len: u8) -> usize {
    if name_len < 24 { 1 } else { 2 }
}
