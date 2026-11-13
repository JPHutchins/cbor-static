#![deny(unsafe_code)]
//! The descriptors themselves: one variant per form a type's values take on the wire.

use crate::engine::{
    AbsentFn, AppendFn, AtFn, BeginFn, DecOps, DecodeFn, EncOps, EncodeFn, EntryFn, FieldPtrFn,
    FinishFn, FreshFn, IndexFn, LenFn, SomeFn, StoreFn, ViewFn, WriteFn,
};
use crate::schema::Layout;

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
    /// `inner` wrapped in the CBOR tag `tag`.
    Tagged {
        ops: &'static EncOps,
        inner: &'static EncKind,
        tag: &'static u64,
    },
    /// A map of `len` entries of `key` and `value`, each found by `entry`.
    Map {
        ops: &'static EncOps,
        key: &'static EncKind,
        value: &'static EncKind,
        len: LenFn,
        entry: EntryFn,
    },
    /// What a deterministic `Map`'s frame encodes in place of each key: its ops pick the key.
    Select { ops: &'static EncOps },
}

impl EncKind {
    #[must_use]
    pub const fn layout(&self) -> Option<&'static Layout> {
        match self {
            Self::Struct { layout, .. } | Self::Enum { layout, .. } => Some(layout),
            Self::Leaf { .. }
            | Self::Array { .. }
            | Self::Option { .. }
            | Self::Chunks { .. }
            | Self::Seq { .. }
            | Self::Tagged { .. }
            | Self::Map { .. }
            | Self::Select { .. } => None,
        }
    }
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
    /// An item taken whole from contiguous input, and checked and written by `store`.
    Raw {
        ops: &'static DecOps,
        store: StoreFn,
        drop: Option<WriteFn>,
    },
    /// `inner`, which the self-describe tag may precede.
    Described {
        ops: &'static DecOps,
        inner: &'static DecKind,
        drop: Option<WriteFn>,
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
    /// The offsets of the key and the value in an entry, and of each entry from the one before.
    pub key_at: usize,
    pub value_at: usize,
    pub stride: usize,
    pub fresh: FreshFn,
    pub drop_key: Option<WriteFn>,
    pub drop_shadow: Option<WriteFn>,
    pub assemble: WriteFn,
    /// The key's encoding, which `Strict` compares bytewise (#12 Q29).
    pub key_encode: &'static EncKind,
}

pub struct DecVariant {
    pub payload: Option<&'static DecKind>,
    pub assemble: WriteFn,
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
            Self::Raw { ops, store, .. } => Self::Raw { ops, store, drop },
            Self::Described { ops, inner, .. } => Self::Described { ops, inner, drop },
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

    /// `inner`, which also writes `default` where its container lacks it: an `Option` kind whose
    /// ops decode every item as `inner`.
    #[must_use]
    pub const fn defaulted(inner: &'static Self, default: WriteFn) -> Self {
        use crate::engine::{DECODE_DEFAULTED, nothing};

        Self::Option {
            ops: &DECODE_DEFAULTED,
            default: Some(default),
            drop: inner.drop_fn(),
            some: inner,
            assemble: nothing,
        }
    }

    /// A `skip`ped field's kind: it decodes no item, and its container writes `default` in its
    /// place.
    #[must_use]
    pub const fn skipped(default: WriteFn, drop: Option<WriteFn>) -> Self {
        use crate::engine::dec_skipped;

        Self::Leaf {
            decode: dec_skipped,
            default: Some(default),
            drop,
        }
    }

    #[must_use]
    pub const fn layout(&self) -> Option<&'static Layout> {
        match self {
            Self::Leaf { .. }
            | Self::Array { .. }
            | Self::Option { .. }
            | Self::Owned { .. }
            | Self::Seq { .. }
            | Self::Raw { .. }
            | Self::Described { .. }
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
            | Self::Raw { drop, .. }
            | Self::Described { drop, .. }
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
            | Self::Raw { .. }
            | Self::Described { .. }
            | Self::Map { .. } => None,
        }
    }
}
