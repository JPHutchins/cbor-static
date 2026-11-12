#![deny(unsafe_code)]
//! Constant descriptors the derives emit. Private: free to change before 1.0.

use crate::engine::{
    AbsentFn, AppendFn, AtFn, BeginFn, DECODE_OPTION, DecOps, DecodeFn, ENCODE_OPTION, EncOps,
    EncodeFn, EntryFn, FieldPtrFn, FinishFn, FreshFn, IndexFn, LenFn, SomeFn, ViewFn, WriteFn,
    absent_option, drop_fn, none, some, some_ptr,
};
use crate::lead::Lead;

/// Encodes through the engine via a constant descriptor; implemented by `#[derive(Encode)]`.
///
/// # Safety
/// The engine trusts the descriptor without checking it against `Self`. `ENCODE` must describe
/// `Self` exactly: every offset is that of a field of `Self`, every kind is the field type's own
/// `Encode::ENCODE` (or, for a `#[cbor(bytes)]` field, its [`ByteString::ENCODE_BYTES`]), and every
/// handler reads only a valid `Self` or field of it. `Stack` must hold
/// `MAX_DEPTH` frames, and `MAX_DEPTH` must be the deepest nesting `ENCODE` reaches.
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
/// `Decode::DECODE` (or, for a `#[cbor(bytes)]` field, its [`ByteString::DECODE_BYTES`]), every
/// handler writes only a valid `Self` or field of it, and the drop handler
/// drops exactly what was written. `Self` borrows the input only through lifetimes `'de`
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
/// # Safety
/// `ENCODE_BYTES` and `DECODE_BYTES` must describe `Self` as the [`Encode`] and [`Decode`] kinds
/// do, with neither reaching a nested frame.
#[allow(unsafe_code)]
pub unsafe trait ByteString: Sized {
    #[doc(hidden)]
    const ENCODE_BYTES: &'static EncKind;
    #[doc(hidden)]
    const DECODE_BYTES: &'static DecKind;
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

/// `repr(C, u8)` keeps `ops` (and a leaf's `encode`) at one offset in every variant, and `absent`
/// at another, so the engine reads them without matching.
#[repr(C, u8)]
pub enum EncKind {
    Leaf {
        encode: EncodeFn,
        absent: Option<AbsentFn>,
    },
    Struct {
        ops: &'static EncOps,
        layout: &'static Layout,
        kinds: &'static [&'static EncKind],
        field_ptr: Option<FieldPtrFn>,
    },
    Enum {
        ops: &'static EncOps,
        layout: &'static Layout,
        payloads: &'static [Option<&'static EncKind>],
        index: IndexFn,
        field_ptr: Option<FieldPtrFn>,
    },
    /// `[T; N]`: `len` elements of `elem`, `stride` bytes apart.
    Array {
        ops: &'static EncOps,
        elem: &'static EncKind,
        len: usize,
        stride: usize,
    },
    /// `Option<T>` of a nested `T`: `None` is null, `Some` is its payload.
    Option {
        ops: &'static EncOps,
        absent: Option<AbsentFn>,
        some: &'static EncKind,
        some_ptr: SomeFn,
    },
    /// [`crate::Chunks`]: a string its ops write chunk by chunk.
    Chunks { ops: &'static EncOps },
    /// A sequence of `elem`s, `stride` bytes apart, that `view` finds.
    Seq {
        ops: &'static EncOps,
        elem: &'static EncKind,
        stride: usize,
        view: ViewFn,
    },
    /// A map of `len` entries of `key` and `value`, each found by `entry`.
    Map {
        ops: &'static EncOps,
        key: &'static EncKind,
        value: &'static EncKind,
        len: LenFn,
        entry: EntryFn,
    },
}

/// `repr(C, u8)` keeps `ops` (or a leaf's `decode`) and `drop` each at one offset in every variant,
/// and `default` at another, so the engine reads them without matching.
#[repr(C, u8)]
pub enum DecKind {
    Leaf {
        decode: DecodeFn,
        default: Option<WriteFn>,
        drop: Option<WriteFn>,
    },
    Struct {
        ops: &'static DecOps,
        layout: &'static Layout,
        drop: Option<WriteFn>,
        kinds: &'static [&'static DecKind],
    },
    Enum {
        ops: &'static DecOps,
        layout: &'static Layout,
        drop: Option<WriteFn>,
        variants: &'static [DecVariant],
    },
    /// `[T; N]`: `len` elements of `elem`, `stride` bytes apart.
    Array {
        ops: &'static DecOps,
        elem: &'static DecKind,
        drop: Option<WriteFn>,
        len: usize,
        stride: usize,
    },
    /// `Option<T>` of a nested `T`: null writes the `default`, `None`; anything else decodes `some`
    /// into the slot, then `assemble` moves it into `Some`.
    Option {
        ops: &'static DecOps,
        default: Option<WriteFn>,
        drop: Option<WriteFn>,
        some: &'static DecKind,
        assemble: WriteFn,
    },
    /// A string copied into the value, chunk by chunk: `begin` writes it empty, `append` copies
    /// each segment of the payload, and `finish` checks the whole.
    Owned {
        ops: &'static DecOps,
        begin: BeginFn,
        drop: Option<WriteFn>,
        append: AppendFn,
        finish: FinishFn,
    },
    /// A sequence of up to a capacity of `elem`s, each decoded in the slot `at` gives.
    Seq {
        ops: &'static DecOps,
        begin: BeginFn,
        drop: Option<WriteFn>,
        elem: &'static DecKind,
        at: AtFn,
    },
    /// A map of up to a capacity of entries of `key` and `value`, decoded through `glue`.
    Map {
        ops: &'static DecOps,
        key: &'static DecKind,
        drop: Option<WriteFn>,
        value: &'static DecKind,
        glue: &'static MapGlue,
    },
}

/// How a map's entries decode in place: into a shadow sequence of `(K, V)` in the map's slot, which
/// `assemble` turns into the map once every entry is in.
pub struct MapGlue {
    /// Writes the empty shadow.
    pub begin: BeginFn,
    /// The shadow's [`AtFn`].
    pub at: AtFn,
    /// The offsets of the key and the value in an entry.
    pub key_at: usize,
    pub value_at: usize,
    pub fresh: FreshFn,
    pub drop_key: Option<WriteFn>,
    pub drop_shadow: Option<WriteFn>,
    pub assemble: WriteFn,
}

pub struct DecVariant {
    pub payload: Option<&'static DecKind>,
    pub assemble: WriteFn,
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

impl DecKind {
    /// This kind for a type that holds exactly the one it describes and drops through `drop`.
    #[must_use]
    pub const fn dropped_by(&self, drop: Option<WriteFn>) -> Self {
        match *self {
            Self::Leaf {
                decode, default, ..
            } => Self::Leaf {
                decode,
                default,
                drop,
            },
            Self::Struct {
                ops, layout, kinds, ..
            } => Self::Struct {
                ops,
                layout,
                drop,
                kinds,
            },
            Self::Enum {
                ops,
                layout,
                variants,
                ..
            } => Self::Enum {
                ops,
                layout,
                drop,
                variants,
            },
            Self::Array {
                ops,
                elem,
                len,
                stride,
                ..
            } => Self::Array {
                ops,
                elem,
                drop,
                len,
                stride,
            },
            Self::Option {
                ops,
                default,
                some,
                assemble,
                ..
            } => Self::Option {
                ops,
                default,
                drop,
                some,
                assemble,
            },
            Self::Owned {
                ops,
                begin,
                append,
                finish,
                ..
            } => Self::Owned {
                ops,
                begin,
                drop,
                append,
                finish,
            },
            Self::Seq {
                ops,
                begin,
                elem,
                at,
                ..
            } => Self::Seq {
                ops,
                begin,
                drop,
                elem,
                at,
            },
            Self::Map {
                ops,
                key,
                value,
                glue,
                ..
            } => Self::Map {
                ops,
                key,
                drop,
                value,
                glue,
            },
        }
    }

    pub(crate) const fn layout(&self) -> Option<&'static Layout> {
        match self {
            Self::Leaf { .. }
            | Self::Array { .. }
            | Self::Option { .. }
            | Self::Owned { .. }
            | Self::Seq { .. }
            | Self::Map { .. } => None,
            Self::Struct { layout, .. } | Self::Enum { layout, .. } => Some(layout),
        }
    }

    pub(crate) const fn drop_fn(&self) -> Option<WriteFn> {
        match self {
            Self::Leaf { drop, .. }
            | Self::Struct { drop, .. }
            | Self::Enum { drop, .. }
            | Self::Array { drop, .. }
            | Self::Option { drop, .. }
            | Self::Owned { drop, .. }
            | Self::Seq { drop, .. }
            | Self::Map { drop, .. } => *drop,
        }
    }

    pub(crate) const fn default_fn(&self) -> Option<WriteFn> {
        match self {
            Self::Leaf { default, .. } | Self::Option { default, .. } => *default,
            Self::Struct { .. }
            | Self::Enum { .. }
            | Self::Array { .. }
            | Self::Owned { .. }
            | Self::Seq { .. }
            | Self::Map { .. } => None,
        }
    }
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
