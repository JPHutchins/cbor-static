use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericParam, Ident, Lifetime, LitStr, Member, Type,
};

const MAX_FIELDS: usize = 64;

pub struct Item {
    pub ident: Ident,
    pub lifetime: Option<Lifetime>,
    pub shape: Shape,
}

pub enum Shape {
    Struct(Vec<Field>),
    Enum { variants: Vec<Variant>, repr: Repr },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Repr {
    Rust,
    U8,
    CU8,
}

pub struct Field {
    pub member: Member,
    pub ty: Type,
    pub name: String,
    pub omit_if_absent: bool,
}

pub struct Variant {
    pub ident: Ident,
    pub name: String,
    pub payload: Payload,
}

pub enum Payload {
    Unit,
    Newtype(Type),
    Struct(Vec<Field>),
}

#[derive(Default)]
struct FieldAttrs {
    rename: Option<String>,
    default: bool,
    skip_if_none: bool,
}

impl Item {
    pub fn parse(ast: &DeriveInput) -> syn::Result<Self> {
        reject_attrs(&ast.attrs, &["cbor", "serde"])?;
        let shape = match &ast.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(named) => Shape::Struct(fields(named.named.iter())?),
                other => {
                    return Err(syn::Error::new(
                        other.span(),
                        "cbor-static: structs need named fields",
                    ));
                }
            },
            Data::Enum(data) if data.variants.len() > MAX_FIELDS => {
                return Err(syn::Error::new(
                    ast.ident.span(),
                    "cbor-static: at most 64 variants per enum",
                ));
            }
            Data::Enum(data) => Shape::Enum {
                variants: data
                    .variants
                    .iter()
                    .map(variant)
                    .collect::<syn::Result<_>>()?,
                repr: repr(&ast.attrs)?,
            },
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

fn variant(variant: &syn::Variant) -> syn::Result<Variant> {
    if let Some((_, discriminant)) = &variant.discriminant {
        return Err(syn::Error::new(
            discriminant.span(),
            "cbor-static: explicit discriminants are unsupported",
        ));
    }
    let attrs = field_attrs(&variant.attrs)?;
    check_name(
        attrs
            .rename
            .as_deref()
            .unwrap_or(&variant.ident.to_string()),
        variant.span(),
    )?;
    if attrs.default || attrs.skip_if_none {
        return Err(syn::Error::new(
            variant.span(),
            "cbor-static: variants accept only `rename`",
        ));
    }
    let payload = match &variant.fields {
        Fields::Unit => Payload::Unit,
        Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => Payload::Newtype(
            unnamed
                .unnamed
                .iter()
                .map(|f| f.ty.clone())
                .next()
                .ok_or_else(|| {
                    syn::Error::new(unnamed.span(), "cbor-static: empty tuple variant")
                })?,
        ),
        Fields::Unnamed(unnamed) => {
            return Err(syn::Error::new(
                unnamed.span(),
                "cbor-static: tuple variants need exactly one field",
            ));
        }
        Fields::Named(named) => Payload::Struct(fields(named.named.iter())?),
    };
    Ok(Variant {
        ident: variant.ident.clone(),
        name: attrs.rename.unwrap_or_else(|| variant.ident.to_string()),
        payload,
    })
}

fn fields<'a>(fields: impl ExactSizeIterator<Item = &'a syn::Field>) -> syn::Result<Vec<Field>> {
    if fields.len() > MAX_FIELDS {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "cbor-static: at most 64 fields per struct",
        ));
    }
    fields
        .map(|field| {
            let attrs = field_attrs(&field.attrs)?;
            let is_option = is_option(&field.ty);
            if (attrs.default || attrs.skip_if_none) && !is_option {
                return Err(syn::Error::new(
                    field.span(),
                    "cbor-static: `default` and `skip_serializing_if` need an `Option` field",
                ));
            }
            let ident = field
                .ident
                .clone()
                .ok_or_else(|| syn::Error::new(field.span(), "unnamed field"))?;
            check_name(
                attrs.rename.as_deref().unwrap_or(&ident.to_string()),
                field.span(),
            )?;
            Ok(Field {
                name: attrs.rename.unwrap_or_else(|| ident.to_string()),
                member: Member::Named(ident),
                ty: field.ty.clone(),
                omit_if_absent: attrs.skip_if_none,
            })
        })
        .collect()
}

fn field_attrs(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("cbor") || a.path().is_ident("serde"))
        .try_fold(FieldAttrs::default(), |mut parsed, attr| {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    parsed.rename = Some(meta.value()?.parse::<LitStr>()?.value());
                } else if meta.path.is_ident("default") {
                    parsed.default = true;
                } else if meta.path.is_ident("skip_serializing_if") {
                    let predicate = meta.value()?.parse::<LitStr>()?;
                    if predicate.value() != "Option::is_none" {
                        return Err(meta.error(
                            "cbor-static: only `skip_serializing_if = \"Option::is_none\"`",
                        ));
                    }
                    parsed.skip_if_none = true;
                } else {
                    return Err(meta.error("cbor-static: unsupported attribute"));
                }
                Ok(())
            })?;
            Ok(parsed)
        })
}

fn reject_attrs(attrs: &[Attribute], names: &[&str]) -> syn::Result<()> {
    attrs
        .iter()
        .find(|a| names.iter().any(|n| a.path().is_ident(n)))
        .map_or(Ok(()), |a| {
            Err(syn::Error::new(
                a.span(),
                "cbor-static: no container attributes yet",
            ))
        })
}

fn is_option(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "Option"))
}

fn check_name(name: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if name.len() > usize::from(u8::MAX) {
        return Err(syn::Error::new(
            span,
            "cbor-static: names are at most 255 bytes",
        ));
    }
    Ok(())
}
