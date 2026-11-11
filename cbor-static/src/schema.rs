#![forbid(unsafe_code)]
//! Constant descriptors the derives emit. Private: free to change before 1.0.

use crate::engine::{Item, Next, Pending};
use crate::error::ErrorKind;

/// Encodes through the engine via a constant descriptor; implemented by `#[derive(Encode)]`.
pub trait Encode {
    #[doc(hidden)]
    const ENCODE: &'static EncKind;
    #[doc(hidden)]
    type Stack;
    #[doc(hidden)]
    const MAX_DEPTH: usize;
}

/// Decodes through the engine via a constant descriptor; implemented by `#[derive(Decode)]`.
pub trait Decode<'de> {
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
pub type DecodeFn = unsafe fn(*mut u8, &Item<'_>) -> Result<Next, ErrorKind>;
pub type WriteFn = unsafe fn(*mut u8);

pub enum EncKind {
    Leaf {
        encode: EncodeFn,
        absent: Option<AbsentFn>,
    },
    Struct {
        layout: &'static Layout,
        kinds: &'static [&'static EncKind],
        field_ptr: Option<FieldPtrFn>,
    },
    Enum {
        layout: &'static Layout,
        payloads: &'static [Option<&'static EncKind>],
        index: IndexFn,
        field_ptr: Option<FieldPtrFn>,
    },
}

pub enum DecKind {
    Leaf {
        decode: DecodeFn,
        default: Option<WriteFn>,
        drop: Option<WriteFn>,
    },
    Struct {
        layout: &'static Layout,
        kinds: &'static [&'static DecKind],
        drop: Option<WriteFn>,
    },
    Enum {
        layout: &'static Layout,
        variants: &'static [DecVariant],
        drop: Option<WriteFn>,
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
            Self::Leaf { .. } => None,
            Self::Struct { layout, .. } | Self::Enum { layout, .. } => Some(layout),
        }
    }

    pub(crate) const fn drop_fn(&self) -> Option<WriteFn> {
        match self {
            Self::Leaf { drop, .. } | Self::Struct { drop, .. } | Self::Enum { drop, .. } => *drop,
        }
    }

    pub(crate) const fn default_fn(&self) -> Option<WriteFn> {
        match self {
            Self::Leaf { default, .. } => *default,
            Self::Struct { .. } | Self::Enum { .. } => None,
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
