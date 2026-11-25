//! Differential oracles: ciborium, minicbor and Python's cbor2 read the same CBOR as cbor-static's
//! `Tokens`, each compared on the data model after its own leg's normalization.

pub mod cbor2;
pub mod hex;
pub mod legs;
pub mod python;
pub mod readers;

/// The longest byte string ciborium reads under tag 2 or 3 as an integer.
pub const SHORT_BIGNUM: usize = 16;

/// One item of the CBOR data model. Head widths are gone; indefinite lengths and chunks stay, for
/// the legs whose reader keeps them.
#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Integer(i128),
    Bytes(Vec<u8>),
    Text(String),
    BytesChunks(Vec<Vec<u8>>),
    TextChunks(Vec<String>),
    Array(Vec<Data>, Length),
    Map(Vec<(Data, Data)>, Length),
    Tag(u64, Box<Data>),
    Simple(u8),
    Bool(bool),
    Null,
    Undefined,
    Float(Float),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Length {
    Definite,
    Indefinite,
}

/// A float as binary64 bits, widened exactly; or a NaN whose reader did not keep its sign and
/// payload, which equals any NaN.
#[derive(Clone, Copy, Debug)]
pub enum Float {
    Bits(u64),
    AnyNan,
}

impl Float {
    pub fn read(value: f64) -> Self {
        Self::Bits(value.to_bits())
    }

    /// A reader's float, whose NaN is any NaN: its reader quiets or rewrites NaNs.
    pub fn lossy_nan(value: f64) -> Self {
        if value.is_nan() {
            Self::AnyNan
        } else {
            Self::read(value)
        }
    }
}

impl PartialEq for Float {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Bits(a), Self::Bits(b)) => a == b,
            (Self::AnyNan, Self::Bits(bits)) | (Self::Bits(bits), Self::AnyNan) => {
                f64::from_bits(bits).is_nan()
            }
            (Self::AnyNan, Self::AnyNan) => true,
        }
    }
}

impl Data {
    /// Every chunked string joined and every length definite: what a reader that builds values
    /// sees.
    pub fn flattened(self) -> Self {
        match self {
            Self::BytesChunks(chunks) => Self::Bytes(chunks.concat()),
            Self::TextChunks(chunks) => Self::Text(chunks.concat()),
            Self::Array(items, _) => Self::Array(
                items.into_iter().map(Self::flattened).collect(),
                Length::Definite,
            ),
            Self::Map(entries, _) => Self::Map(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.flattened(), value.flattened()))
                    .collect(),
                Length::Definite,
            ),
            Self::Tag(tag, content) => Self::Tag(tag, Box::new(content.flattened())),
            leaf @ (Self::Integer(_)
            | Self::Bytes(_)
            | Self::Text(_)
            | Self::Simple(_)
            | Self::Bool(_)
            | Self::Null
            | Self::Undefined
            | Self::Float(_)) => leaf,
        }
    }

    fn any(&self, matches: &impl Fn(&Self) -> bool) -> bool {
        matches(self)
            || match self {
                Self::Array(items, _) => items.iter().any(|item| item.any(matches)),
                Self::Map(entries, _) => entries
                    .iter()
                    .any(|(key, value)| key.any(matches) || value.any(matches)),
                Self::Tag(_, content) => content.any(matches),
                Self::Integer(_)
                | Self::Bytes(_)
                | Self::Text(_)
                | Self::BytesChunks(_)
                | Self::TextChunks(_)
                | Self::Simple(_)
                | Self::Bool(_)
                | Self::Null
                | Self::Undefined
                | Self::Float(_) => false,
            }
    }

    pub fn has_undefined(&self) -> bool {
        self.any(&|data| matches!(data, Self::Undefined))
    }

    pub fn has_any_nan(&self) -> bool {
        self.any(&|data| matches!(data, Self::Float(Float::AnyNan)))
    }

    pub fn has_indefinite(&self) -> bool {
        self.any(&|data| {
            matches!(
                data,
                Self::BytesChunks(_)
                    | Self::TextChunks(_)
                    | Self::Array(_, Length::Indefinite)
                    | Self::Map(_, Length::Indefinite)
            )
        })
    }

    /// Whether any item is a tag 2 or 3 over definite bytes short enough for ciborium to read as an
    /// integer.
    pub fn has_short_bignum(&self) -> bool {
        self.any(&|data| {
            matches!(
                data,
                Self::Tag(2 | 3, content)
                    if matches!(&**content, Self::Bytes(bytes) if bytes.len() <= SHORT_BIGNUM)
            )
        })
    }

    /// Every NaN as any NaN, for a reader that rewrites them.
    pub fn nan_is_any_nan(self) -> Self {
        match self {
            Self::Float(Float::Bits(bits)) => Self::Float(Float::lossy_nan(f64::from_bits(bits))),
            Self::Array(items, length) => Self::Array(
                items.into_iter().map(Self::nan_is_any_nan).collect(),
                length,
            ),
            Self::Map(entries, length) => Self::Map(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.nan_is_any_nan(), value.nan_is_any_nan()))
                    .collect(),
                length,
            ),
            Self::Tag(tag, content) => Self::Tag(tag, Box::new(content.nan_is_any_nan())),
            leaf @ (Self::Integer(_)
            | Self::Bytes(_)
            | Self::Text(_)
            | Self::BytesChunks(_)
            | Self::TextChunks(_)
            | Self::Simple(_)
            | Self::Bool(_)
            | Self::Null
            | Self::Undefined
            | Self::Float(Float::AnyNan)) => leaf,
        }
    }
}
