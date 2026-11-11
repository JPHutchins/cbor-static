#![deny(unsafe_code)]
//! Constant descriptors the derives emit. Private: free to change before 1.0.

use crate::engine::{DecOps, EncOps, Item, Next, Pending};
use crate::error::ErrorKind;

/// Encodes through the engine via a constant descriptor; implemented by `#[derive(Encode)]`.
///
/// # Safety
/// The engine trusts the descriptor without checking it against `Self`. `ENCODE` must describe
/// `Self` exactly: every offset is that of a field of `Self`, every kind is the field type's own
/// `Encode::ENCODE`, and every handler reads only a valid `Self` or field of it. `Stack` must hold
/// `MAX_DEPTH` frames, and `MAX_DEPTH` must be the deepest nesting `ENCODE` reaches.
#[allow(unsafe_code)]
pub unsafe trait Encode {
    #[doc(hidden)]
    const ENCODE: &'static EncKind;
    #[doc(hidden)]
    type Stack;
    #[doc(hidden)]
    const MAX_DEPTH: usize;
}

/// Decodes through the engine via a constant descriptor; implemented by `#[derive(Decode)]`.
///
/// # Safety
/// The engine trusts the descriptor without checking it against `Self`. `DECODE` must describe
/// `Self` exactly: every offset is that of a field of `Self`, every kind is the field type's own
/// `Decode::DECODE`, every handler writes only a valid `Self` or field of it, and the drop handler
/// drops exactly what was written. `Self` borrows the input only through lifetimes `'de`
/// outlives. `Stack` must hold `MAX_DEPTH` frames, and `MAX_DEPTH` must be the deepest nesting
/// `DECODE` reaches.
#[allow(unsafe_code)]
pub unsafe trait Decode<'de> {
    #[doc(hidden)]
    const DECODE: &'static DecKind;
    #[doc(hidden)]
    type Stack;
    #[doc(hidden)]
    const MAX_DEPTH: usize;
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

pub type EncodeFn = unsafe fn(*const u8, &mut Pending);
pub type AbsentFn = unsafe fn(*const u8) -> bool;
pub type IndexFn = unsafe fn(*const u8) -> u8;
pub type FieldPtrFn = unsafe fn(*const u8, u8) -> *const u8;
/// The payload of an `Option`, or null for `None`.
pub type SomeFn = unsafe fn(*const u8) -> *const u8;
pub type DecodeFn = unsafe fn(*mut u8, &Item<'_>) -> Result<Next, ErrorKind>;
pub type WriteFn = unsafe fn(*mut u8);

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
    pub(crate) const fn layout(&self) -> Option<&'static Layout> {
        match self {
            Self::Leaf { .. } | Self::Array { .. } | Self::Option { .. } => None,
            Self::Struct { layout, .. } | Self::Enum { layout, .. } => Some(layout),
        }
    }

    pub(crate) const fn drop_fn(&self) -> Option<WriteFn> {
        match self {
            Self::Leaf { drop, .. }
            | Self::Struct { drop, .. }
            | Self::Enum { drop, .. }
            | Self::Array { drop, .. }
            | Self::Option { drop, .. } => *drop,
        }
    }

    pub(crate) const fn default_fn(&self) -> Option<WriteFn> {
        match self {
            Self::Leaf { default, .. } | Self::Option { default, .. } => *default,
            Self::Struct { .. } | Self::Enum { .. } | Self::Array { .. } => None,
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
