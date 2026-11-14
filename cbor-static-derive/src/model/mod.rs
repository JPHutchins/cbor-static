mod attrs;
mod case;
mod enums;
mod fields;

use proc_macro2::Span;
use syn::spanned::Spanned as _;
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, GenericParam, Generics, Ident, Lifetime, Member,
    Path, Type,
};

use attrs::StructAttrs;
use enums::enumeration;
use fields::{bare, fields_of, holds, lone, positional};

const MAX_FIELDS: usize = 64;

pub struct Item {
    pub ident: Ident,
    /// The one lifetime the item may borrow input for.
    pub lifetime: Option<Lifetime>,
    /// Every parameter, the lifetime included (R-127).
    pub generics: Generics,
    pub shape: Shape,
}

/// The wire shape of a struct follows serde (D-08): a unit struct is `null`, a newtype struct its
/// field, and a tuple struct an array.
pub enum Shape {
    Struct {
        fields: Vec<Field>,
        form: StructForm,
        order: Mode,
    },
    Unit,
    /// A newtype or `transparent` struct, which a key order makes a deterministic root of its
    /// field.
    Newtype {
        field: Box<Field>,
        order: Mode,
    },
    Enum {
        variants: Vec<Variant>,
        repr: Repr,
        form: EnumForm,
        offsets: Offsets,
        order: Mode,
    },
}

/// The key order a type writes its own maps in (D-01, D-11b).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Declaration order.
    #[default]
    Declared,
    /// `deterministic`: RFC 8949 §4.2.1, bytewise by the keys' encodings, sorted at compile time.
    Bytewise,
    /// `length_first`: RFC 8949 §4.2.3, shorter encodings first.
    LengthFirst,
}

/// Where a primitive-`repr` enum's payload offsets come from (D-21, R-129).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Offsets {
    /// `offset_of!` on `#[repr(C)]` shadow structs laid out as RFC 2195 lays out the variants.
    Shadows,
    /// `offset_of!` on the enum's variants themselves (`#[cbor(offset_of_enum)]`), which needs
    /// `#![feature(offset_of_enum)]` in the deriving crate.
    Enum,
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
    /// A `flatten`ed field, which has no key of its own: its type's entries join its struct's map
    /// in its place (#14 Q34).
    Flattened,
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
                    let (transparent, order) = tuple_attrs(&ast.attrs)?;
                    match (
                        <[Field; 1]>::try_from(positional(unnamed)?),
                        transparent,
                        order,
                    ) {
                        (Ok([field]), _, order) => Shape::Newtype {
                            field: Box::new(field),
                            order: order.unwrap_or_default(),
                        },
                        (Err(fields), false, order) => Shape::Struct {
                            fields,
                            form: StructForm::Array,
                            order: order.unwrap_or_default(),
                        },
                        (Err(_), true, _) => return Err(not_one_field()),
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
        holds_not_itself(&ast.ident, &shape)?;
        flattens_no_parameter(&ast.generics, &shape)?;
        Ok(Self {
            ident: ast.ident.clone(),
            lifetime: lifetime(ast)?,
            generics: ast.generics.clone(),
            shape,
        })
    }
}

/// D-14: a type that holds itself has no bounded depth, and recursive types are unsupported. The
/// derive names the field, where rustc would report E0072 or E0275, or, through a raw view such as
/// `ArrayRef`, compile a decoder whose nested decoders recurse as deep as the input.
fn holds_not_itself(ident: &Ident, shape: &Shape) -> syn::Result<()> {
    match shape {
        Shape::Struct { fields, .. } => fields.iter().collect(),
        Shape::Unit => Vec::new(),
        Shape::Newtype { field, .. } => vec![field.as_ref()],
        Shape::Enum { variants, .. } => variants
            .iter()
            .flat_map(|variant| match &variant.payload {
                Payload::Unit => Vec::new(),
                Payload::Newtype(field) => vec![field.as_ref()],
                Payload::Struct(fields) => fields.iter().collect(),
            })
            .collect::<Vec<&Field>>(),
    }
    .into_iter()
    .find(|field| holds(&field.ty, ident))
    .map_or(Ok(()), |field| {
        Err(syn::Error::new(
            field.ty.span(),
            format!("cbor-static: `{ident}` holds itself; recursive types are unsupported"),
        ))
    })
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
        order: attrs.order.unwrap_or_default(),
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
                order,
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
            lone(field, member).map(|field| Shape::Newtype {
                field: Box::new(field),
                order: order.unwrap_or_default(),
            })
        }
        (_, 1, _) => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: a `transparent` struct takes no other struct attribute",
        )),
        _ => Err(not_one_field()),
    }
}

/// A tuple struct takes `transparent`, which it already is when it has one field, or a key order.
fn tuple_attrs(attrs: &[Attribute]) -> syn::Result<(bool, Option<Mode>)> {
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
            order,
        } => Ok((transparent, order)),
        _ => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: a tuple struct takes only `transparent`, `deterministic` or \
             `length_first`",
        )),
    }
}

fn not_one_field() -> syn::Error {
    syn::Error::new(
        Span::call_site(),
        "cbor-static: a `transparent` struct has exactly one field",
    )
}

/// The item's one lifetime, if any: type and const parameters may join it (R-127).
fn lifetime(ast: &DeriveInput) -> syn::Result<Option<Lifetime>> {
    ast.generics
        .params
        .iter()
        .try_fold(None, |found, param| match (found, param) {
            (found, GenericParam::Type(_) | GenericParam::Const(_)) => Ok(found),
            (None, GenericParam::Lifetime(def)) if def.bounds.is_empty() => {
                Ok(Some(def.lifetime.clone()))
            }
            (_, GenericParam::Lifetime(other)) => Err(syn::Error::new(
                other.span(),
                "cbor-static: at most one unbounded lifetime parameter is supported",
            )),
        })
}

/// A flattened field's entries are copied into its struct's at compile time, into arrays whose
/// shapes the field's type names; when that type is one of the item's own parameters, stable Rust
/// cannot borrow them for `'static` (#25).
fn flattens_no_parameter(generics: &Generics, shape: &Shape) -> syn::Result<()> {
    let Shape::Struct { fields, .. } = shape else {
        return Ok(());
    };
    fields
        .iter()
        .filter(|field| matches!(field.key, Key::Flattened))
        .find(|field| {
            generics
                .type_params()
                .any(|param| matches!(&field.ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident(&param.ident)))
        })
        .map_or(Ok(()), |field| {
            Err(syn::Error::new(
                field.ty.span(),
                "cbor-static: a type parameter cannot be flattened yet; see \
                 https://github.com/JPHutchins/cbor-static/issues/25",
            ))
        })
}
