mod attrs;
mod case;
mod enums;
mod fields;

use proc_macro2::Span;
use syn::spanned::Spanned as _;
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, GenericParam, Ident, Lifetime, Member, Path, Type,
};

use attrs::StructAttrs;
use enums::enumeration;
use fields::{bare, fields_of, lone, positional};

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
    /// A `skip`ped field, which no key names: never written, and given its default on decode.
    Skipped,
}

/// What decoding writes into a field that its container lacks (D-08, #13 Q31).
#[derive(Clone, Default)]
pub enum Fallback {
    /// What the field's type writes: `None` for an `Option`; for any other type, nothing, so the
    /// field is required.
    #[default]
    Type,
    /// `default`: `T::default()`.
    Default,
    /// `default = "path"`: what `path()` returns.
    Path(Path),
}

pub struct Field {
    pub member: Member,
    pub ty: Type,
    pub key: Key,
    pub fallback: Fallback,
    pub omit_if_absent: bool,
    /// `#[cbor(bytes)]`: the type's byte-string encoding, not its own (#12 Q21).
    pub bytes: bool,
}

pub struct Variant {
    pub ident: Ident,
    pub name: String,
    /// Whether the variant names itself with `rename`.
    pub renamed: bool,
    pub cbor_tag: Option<u64>,
    /// The discriminant written as `= expr`; others follow the one before, as in Rust.
    pub discriminant: Option<Expr>,
    pub payload: Payload,
}

pub enum Payload {
    Unit,
    /// A positional field, which takes `#[cbor(bytes)]` alone, as a tuple struct's fields do.
    Newtype(Box<Field>),
    Struct(Vec<Field>),
}

impl Item {
    pub fn parse(ast: &DeriveInput) -> syn::Result<Self> {
        let shape = match &ast.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(named) => structure(&ast.attrs, named.named.iter())?,
                Fields::Unnamed(unnamed) => {
                    let transparent = tuple_attrs(&ast.attrs)?;
                    match (<[Field; 1]>::try_from(positional(unnamed)?), transparent) {
                        (Ok([field]), _) => Shape::Newtype(Box::new(field)),
                        (Err(fields), false) => Shape::Struct {
                            fields,
                            form: StructForm::Array,
                        },
                        (Err(_), true) => return Err(not_one_field()),
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
    mut fields: impl ExactSizeIterator<Item = &'a syn::Field>,
) -> syn::Result<Shape> {
    let attrs = attrs::parse(attrs)?
        .into_iter()
        .try_fold(StructAttrs::default(), StructAttrs::with)?;
    if attrs.transparent {
        return transparent(&attrs, &mut fields);
    }
    let form = match (
        attrs.array,
        &attrs.keys,
        attrs.deny_unknown_fields,
        attrs.rename_all,
    ) {
        (true, None, false, None) => StructForm::Array,
        (true, ..) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "cbor-static: `array` structs take neither `keys`, `deny_unknown_fields` \
                 nor `rename_all`",
            ));
        }
        (false, Some(_), _, Some(_)) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "cbor-static: `keys` names every key, so `rename_all` has nothing to rename",
            ));
        }
        (false, _, deny, _) => StructForm::Map {
            unknown: if deny { Unknown::Deny } else { Unknown::Skip },
        },
    };
    Ok(Shape::Struct {
        fields: fields_of(fields, form, attrs.keys.as_ref(), attrs.rename_all)?,
        form,
    })
}

/// A named struct with `transparent` (Q8): exactly one field, which it is on the wire, and no other
/// struct attribute.
fn transparent<'a>(
    attrs: &StructAttrs,
    fields: &mut impl ExactSizeIterator<Item = &'a syn::Field>,
) -> syn::Result<Shape> {
    match (attrs, fields.len(), fields.next()) {
        (
            StructAttrs {
                keys: None,
                array: false,
                deny_unknown_fields: false,
                rename_all: None,
                transparent: true,
            },
            1,
            Some(field),
        ) => {
            let member = Member::Named(
                field
                    .ident
                    .clone()
                    .ok_or_else(|| syn::Error::new(field.span(), "unnamed field"))?,
            );
            lone(field, member).map(|field| Shape::Newtype(Box::new(field)))
        }
        (_, 1, _) => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: a `transparent` struct takes no other struct attribute",
        )),
        _ => Err(not_one_field()),
    }
}

/// A tuple struct takes `transparent` alone, which it already is when it has one field.
fn tuple_attrs(attrs: &[Attribute]) -> syn::Result<bool> {
    match attrs::parse(attrs)?
        .into_iter()
        .try_fold(StructAttrs::default(), StructAttrs::with)?
    {
        StructAttrs {
            keys: None,
            array: false,
            deny_unknown_fields: false,
            rename_all: None,
            transparent,
        } => Ok(transparent),
        _ => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: a tuple struct takes only `transparent`",
        )),
    }
}

fn not_one_field() -> syn::Error {
    syn::Error::new(
        Span::call_site(),
        "cbor-static: a `transparent` struct has exactly one field",
    )
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
