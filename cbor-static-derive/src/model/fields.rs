use proc_macro2::Span;
use quote::format_ident;
use syn::ext::IdentExt;
use syn::spanned::Spanned as _;
use syn::{Attribute, Fields, FieldsUnnamed, Ident, Index, Member, Path, Type};

use super::attrs::{self, FieldAttrs, KeyLit, VariantAttrs};
use super::case::Case;
use super::{Fallback, Field, Key, MAX_FIELDS, Payload, StructForm, Unknown, Variant};

/// A variant, named by its `rename`, else by its identifier under the enum's `rename_all`, which
/// by default keeps it as written.
pub fn variant(variant: &syn::Variant, rename_all: Option<Case>) -> syn::Result<Variant> {
    let attrs = attrs::parse(&variant.attrs)?
        .into_iter()
        .try_fold(VariantAttrs::default(), VariantAttrs::with)?;
    let renamed = attrs.rename.is_some();
    let name = attrs.rename.unwrap_or_else(|| {
        rename_all
            .unwrap_or(Case::Pascal)
            .variant(&variant.ident.unraw().to_string())
    });
    check_name(&name, variant.span())?;
    let payload = match &variant.fields {
        Fields::Unit => Payload::Unit,
        Fields::Unnamed(unnamed) => match <[Field; 1]>::try_from(positional(unnamed)?) {
            Ok([field]) => Payload::Newtype(Box::new(field)),
            Err(_) => {
                return Err(syn::Error::new(
                    unnamed.span(),
                    "cbor-static: tuple variants need exactly one field",
                ));
            }
        },
        Fields::Named(named) => Payload::Struct(fields_of(
            named.named.iter(),
            StructForm::Map {
                unknown: Unknown::Skip,
            },
            None,
            None,
        )?),
    };
    Ok(Variant {
        ident: variant.ident.clone(),
        name,
        renamed,
        cbor_tag: attrs.cbor_tag,
        discriminant: variant
            .discriminant
            .as_ref()
            .map(|(_, discriminant)| discriminant.clone()),
        payload,
    })
}

/// A named struct's or struct variant's fields, in their wire order: as declared, with the `skip`ped
/// ones last, which an `array` struct needs and a map does not mind.
pub fn fields_of<'a>(
    fields: impl ExactSizeIterator<Item = &'a syn::Field>,
    form: StructForm,
    keys: Option<&Path>,
    rename_all: Option<Case>,
) -> syn::Result<Vec<Field>> {
    within_limit(fields.len())?;
    let (kept, skipped): (Vec<Field>, Vec<Field>) = fields
        .map(|field| {
            let attrs = attrs::parse(&field.attrs)?
                .into_iter()
                .try_fold(FieldAttrs::default(), FieldAttrs::with)?;
            let is_option = is_option(&field.ty);
            if attrs.skip_if_none && !is_option {
                return Err(syn::Error::new(
                    field.span(),
                    "cbor-static: `skip_serializing_if` needs an `Option` field",
                ));
            }
            let ident = field
                .ident
                .clone()
                .ok_or_else(|| syn::Error::new(field.span(), "unnamed field"))?;
            if attrs.skip {
                return skip(field, ident, attrs);
            }
            let key = key(form, keys, rename_all, &ident, attrs.key, attrs.rename)
                .map_err(|message| syn::Error::new(field.span(), message))?;
            if let Key::Text(text) = &key {
                check_name(text, field.span())?;
            }
            Ok(Field {
                member: Member::Named(ident),
                ty: field.ty.clone(),
                key,
                fallback: match (attrs.default, is_option) {
                    (Fallback::Default, true) => Fallback::Type,
                    (fallback, _) => fallback,
                },
                omit_if_absent: match form {
                    StructForm::Map { .. } => attrs.skip_if_none,
                    StructForm::Array => is_option,
                },
                bytes: attrs.bytes,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?
        .into_iter()
        .partition(|field| !matches!(field.key, Key::Skipped));
    Ok(kept.into_iter().chain(skipped).collect())
}

/// A field's key: `keys`' variant, its own `key` or `rename`, or its identifier under `rename_all`;
/// else what is wrong with its attributes.
fn key(
    form: StructForm,
    keys: Option<&Path>,
    rename_all: Option<Case>,
    ident: &Ident,
    key: Option<KeyLit>,
    rename: Option<String>,
) -> Result<Key, &'static str> {
    match (form, keys, key, rename) {
        (StructForm::Array, _, None, None) => Ok(Key::Position),
        (StructForm::Array, ..) => {
            Err("cbor-static: `array` fields are positional and take no `key` or `rename`")
        }
        (_, Some(label), None, None) => Ok(Key::Label {
            label: label.clone(),
            variant: camel(ident),
        }),
        (_, Some(_), ..) => {
            Err("cbor-static: `keys` names every key; drop this field's `key` or `rename`")
        }
        (_, None, Some(_), Some(_)) => {
            Err("cbor-static: a field takes `key` or `rename`, not both")
        }
        (_, None, Some(KeyLit::Int(int)), None) => Ok(Key::Int(int)),
        (_, None, Some(KeyLit::Text(text)), None) | (_, None, None, Some(text)) => {
            Ok(Key::Text(text))
        }
        (_, None, None, None) => Ok(Key::Text(
            rename_all
                .unwrap_or(Case::Snake)
                .field(&ident.unraw().to_string()),
        )),
    }
}

/// A `skip`ped field (#13 Q31): never written, never matched, and given `T::default()` or its
/// `default = "path"` on decode, so it takes nothing else.
fn skip(field: &syn::Field, ident: Ident, attrs: FieldAttrs) -> syn::Result<Field> {
    match attrs {
        FieldAttrs {
            rename: None,
            key: None,
            skip_if_none: false,
            bytes: false,
            skip: _,
            default,
        } => Ok(Field {
            member: Member::Named(ident),
            ty: field.ty.clone(),
            key: Key::Skipped,
            fallback: match default {
                Fallback::Type => Fallback::Default,
                written => written,
            },
            omit_if_absent: true,
            bytes: false,
        }),
        _ => Err(syn::Error::new(
            field.span(),
            "cbor-static: a skipped field takes only `default`",
        )),
    }
}

/// A tuple or newtype struct's fields, in order: never omitted, so every one is written, as serde
/// does. Being positional, they take `#[cbor(bytes)]` and nothing else.
pub fn positional(fields: &FieldsUnnamed) -> syn::Result<Vec<Field>> {
    within_limit(fields.unnamed.len())?;
    fields
        .unnamed
        .iter()
        .enumerate()
        .map(|(index, field)| {
            match attrs::parse(&field.attrs)?
                .into_iter()
                .try_fold(FieldAttrs::default(), FieldAttrs::with)?
            {
                FieldAttrs {
                    rename: None,
                    default: Fallback::Type,
                    skip_if_none: false,
                    skip: false,
                    key: None,
                    bytes,
                } => Ok(Field {
                    member: Member::Unnamed(Index::from(index)),
                    ty: field.ty.clone(),
                    key: Key::Position,
                    fallback: Fallback::Type,
                    omit_if_absent: false,
                    bytes,
                }),
                _ => Err(syn::Error::new(
                    field.span(),
                    "cbor-static: a positional field takes only `#[cbor(bytes)]`",
                )),
            }
        })
        .collect()
}

fn within_limit(fields: usize) -> syn::Result<()> {
    if fields > MAX_FIELDS {
        return Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: at most 64 fields per struct",
        ));
    }
    Ok(())
}

/// Rejects every `#[cbor]` and `#[serde]` attribute in `attrs`.
pub fn bare(attrs: &[Attribute], position: &str) -> syn::Result<()> {
    attrs::parse(attrs)?.first().map_or(Ok(()), |attr| {
        Err(syn::Error::new(
            attr.span,
            format!("cbor-static: {position} takes no attributes"),
        ))
    })
}

/// The `keys = Label` variant a field maps to: its name in `UpperCamelCase`.
fn camel(field: &Ident) -> Ident {
    format_ident!(
        "{}",
        Case::Pascal.field(&field.unraw().to_string()),
        span = field.span()
    )
}

fn is_option(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "Option"))
}

pub fn check_name(name: &str, span: Span) -> syn::Result<()> {
    if name.len() > usize::from(u8::MAX) {
        return Err(syn::Error::new(
            span,
            "cbor-static: names are at most 255 bytes",
        ));
    }
    Ok(())
}
