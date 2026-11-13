mod attrs;
mod fields;

use proc_macro2::Span;
use syn::spanned::Spanned as _;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericParam, Ident, Lifetime, Member, Path, Type,
};

use attrs::{EnumAttrs, StructAttrs};
use fields::{bare, check_name, fields_of, positional, variant};

const MAX_FIELDS: usize = 64;

pub struct Item {
    pub ident: Ident,
    pub lifetime: Option<Lifetime>,
    pub shape: Shape,
}

/// The wire shape of a struct follows serde (D-08): a unit struct is `null`, a newtype struct its
/// field, and a tuple struct an array.
pub enum Shape {
    Struct {
        fields: Vec<Field>,
        form: StructForm,
    },
    Unit,
    Newtype(Box<Field>),
    Enum {
        variants: Vec<Variant>,
        repr: Repr,
        form: EnumForm,
    },
}

/// How a struct is laid out on the wire (D-09).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StructForm {
    Map { unknown: Unknown },
    Array,
}

/// What decoding does with a map entry no field claims (D-12).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    Skip,
    Deny,
}

/// How an enum identifies its variant on the wire (D-10).
pub enum EnumForm {
    External,
    Index,
    CborTag,
    Internal { tag: String },
    Adjacent { tag: String, content: String },
    Untagged,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Repr {
    Rust,
    U8,
    CU8,
}

/// A field's map key (D-09).
pub enum Key {
    Text(String),
    Int(i64),
    /// Variant `variant` of the C-like enum `label`, whose discriminant is the key.
    Label {
        label: Path,
        variant: Ident,
    },
    /// A tuple or `array` struct's field, identified by its position alone.
    Position,
}

pub struct Field {
    pub member: Member,
    pub ty: Type,
    pub key: Key,
    pub omit_if_absent: bool,
    /// `#[cbor(bytes)]`: the type's byte-string encoding, not its own (#12 Q21).
    pub bytes: bool,
}

pub struct Variant {
    pub ident: Ident,
    pub name: String,
    pub cbor_tag: Option<u64>,
    pub payload: Payload,
}

pub enum Payload {
    Unit,
    Newtype(Type),
    Struct(Vec<Field>),
}

impl Item {
    pub fn parse(ast: &DeriveInput) -> syn::Result<Self> {
        let shape = match &ast.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(named) => structure(&ast.attrs, named.named.iter())?,
                Fields::Unnamed(unnamed) => {
                    bare(&ast.attrs, "a tuple struct")?;
                    match <[Field; 1]>::try_from(positional(unnamed)?) {
                        Ok([field]) => Shape::Newtype(Box::new(field)),
                        Err(fields) => Shape::Struct {
                            fields,
                            form: StructForm::Array,
                        },
                    }
                }
                Fields::Unit => {
                    bare(&ast.attrs, "a unit struct")?;
                    Shape::Unit
                }
            },
            Data::Enum(data) if data.variants.len() > MAX_FIELDS => {
                return Err(syn::Error::new(
                    ast.ident.span(),
                    "cbor-static: at most 64 variants per enum",
                ));
            }
            Data::Enum(data) => enumeration(&ast.attrs, data)?,
            Data::Union(data) => {
                return Err(syn::Error::new(
                    data.union_token.span,
                    "cbor-static: unions are unsupported",
                ));
            }
        };
        Ok(Self {
            ident: ast.ident.clone(),
            lifetime: lifetime(ast)?,
            shape,
        })
    }
}

fn structure<'a>(
    attrs: &[Attribute],
    fields: impl ExactSizeIterator<Item = &'a syn::Field>,
) -> syn::Result<Shape> {
    let attrs = attrs::parse(attrs)?
        .into_iter()
        .try_fold(StructAttrs::default(), StructAttrs::with)?;
    let form = match (attrs.array, &attrs.keys, attrs.deny_unknown_fields) {
        (true, None, false) => StructForm::Array,
        (true, ..) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "cbor-static: `array` structs take neither `keys` nor `deny_unknown_fields`",
            ));
        }
        (false, _, deny) => StructForm::Map {
            unknown: if deny { Unknown::Deny } else { Unknown::Skip },
        },
    };
    Ok(Shape::Struct {
        fields: fields_of(fields, form, attrs.keys.as_ref())?,
        form,
    })
}

fn enumeration(item_attrs: &[Attribute], data: &syn::DataEnum) -> syn::Result<Shape> {
    let attrs = attrs::parse(item_attrs)?
        .into_iter()
        .try_fold(EnumAttrs::default(), EnumAttrs::with)?;
    let variants = data
        .variants
        .iter()
        .map(variant)
        .collect::<syn::Result<Vec<_>>>()?;
    let form = match (
        attrs,
        variants.iter().filter(|v| v.cbor_tag.is_some()).count(),
    ) {
        (
            EnumAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: false,
            },
            0,
        ) => EnumForm::External,
        (
            EnumAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: false,
            },
            all,
        ) if all == variants.len() => EnumForm::CborTag,
        (
            EnumAttrs {
                index: true,
                tag: None,
                content: None,
                untagged: false,
            },
            0,
        ) => EnumForm::Index,
        (
            EnumAttrs {
                index: false,
                tag: Some(tag),
                content: None,
                untagged: false,
            },
            0,
        ) => EnumForm::Internal { tag },
        (
            EnumAttrs {
                index: false,
                tag: Some(tag),
                content: Some(content),
                untagged: false,
            },
            0,
        ) => EnumForm::Adjacent { tag, content },
        (
            EnumAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: true,
            },
            0,
        ) => EnumForm::Untagged,
        _ => {
            return Err(syn::Error::new(
                Span::call_site(),
                "cbor-static: one enum representation: `index`, `tag` (with `content`), \
                 `untagged`, or `cbor_tag` on every variant",
            ));
        }
    };
    if let EnumForm::Internal { tag } | EnumForm::Adjacent { tag, .. } = &form {
        check_name(tag, Span::call_site())?;
    }
    if let EnumForm::Adjacent { content, .. } = &form {
        check_name(content, Span::call_site())?;
    }
    if let (EnumForm::CborTag, Some(unit)) = (
        &form,
        variants.iter().find(|v| matches!(v.payload, Payload::Unit)),
    ) {
        return Err(syn::Error::new(
            unit.ident.span(),
            "cbor-static: a `cbor_tag` variant needs a payload",
        ));
    }
    Ok(Shape::Enum {
        variants,
        repr: repr(item_attrs)?,
        form,
    })
}

fn lifetime(ast: &DeriveInput) -> syn::Result<Option<Lifetime>> {
    ast.generics
        .params
        .iter()
        .try_fold(None, |found, param| match (found, param) {
            (None, GenericParam::Lifetime(def)) if def.bounds.is_empty() => {
                Ok(Some(def.lifetime.clone()))
            }
            (_, other) => Err(syn::Error::new(
                other.span(),
                "cbor-static: at most one unbounded lifetime parameter is supported",
            )),
        })
}

fn repr(attrs: &[Attribute]) -> syn::Result<Repr> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("repr"))
        .try_fold(Repr::Rust, |_, attr| {
            let mut words = Vec::new();
            attr.parse_nested_meta(|meta| {
                words.push(
                    meta.path
                        .get_ident()
                        .map_or_else(String::new, ToString::to_string),
                );
                Ok(())
            })?;
            match words
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
            {
                ["u8"] => Ok(Repr::U8),
                ["C", "u8"] | ["u8", "C"] => Ok(Repr::CU8),
                _ => Err(syn::Error::new(
                    attr.span(),
                    "cbor-static: enums support repr(u8) or repr(C, u8)",
                )),
            }
        })
}
