use proc_macro2::Span;
use quote::format_ident;
use syn::ext::IdentExt;
use syn::spanned::Spanned as _;
use syn::{Attribute, Fields, Ident, Index, Member, Path, Type};

use super::attrs::{self, FieldAttrs, KeyLit, VariantAttrs};
use super::{Field, Key, MAX_FIELDS, Payload, StructForm, Unknown, Variant};

pub fn variant(variant: &syn::Variant) -> syn::Result<Variant> {
    if let Some((_, discriminant)) = &variant.discriminant {
        return Err(syn::Error::new(
            discriminant.span(),
            "cbor-static: explicit discriminants are unsupported",
        ));
    }
    let attrs = attrs::parse(&variant.attrs)?
        .into_iter()
        .try_fold(VariantAttrs::default(), VariantAttrs::with)?;
    let name = attrs
        .rename
        .unwrap_or_else(|| variant.ident.unraw().to_string());
    check_name(&name, variant.span())?;
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
        Fields::Named(named) => Payload::Struct(fields_of(
            named.named.iter(),
            StructForm::Map {
                unknown: Unknown::Skip,
            },
            None,
        )?),
    };
    Ok(Variant {
        ident: variant.ident.clone(),
        name,
        cbor_tag: attrs.cbor_tag,
        payload,
    })
}

pub fn fields_of<'a>(
    fields: impl ExactSizeIterator<Item = &'a syn::Field>,
    form: StructForm,
    keys: Option<&Path>,
) -> syn::Result<Vec<Field>> {
    within_limit(fields.len())?;
    fields
        .map(|field| {
            let attrs = attrs::parse(&field.attrs)?
                .into_iter()
                .try_fold(FieldAttrs::default(), FieldAttrs::with)?;
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
            let key = match (form, keys, attrs.key, attrs.rename) {
                (StructForm::Array, _, Some(_), _) => {
                    return Err(syn::Error::new(
                        field.span(),
                        "cbor-static: `array` fields are positional and take no `key`",
                    ));
                }
                (_, Some(label), None, None) => Key::Label {
                    label: label.clone(),
                    variant: camel(&ident),
                },
                (_, Some(_), ..) => {
                    return Err(syn::Error::new(
                        field.span(),
                        "cbor-static: `keys` names every key; drop this field's `key` or `rename`",
                    ));
                }
                (_, None, Some(_), Some(_)) => {
                    return Err(syn::Error::new(
                        field.span(),
                        "cbor-static: a field takes `key` or `rename`, not both",
                    ));
                }
                (_, None, Some(KeyLit::Int(int)), None) => Key::Int(int),
                (_, None, Some(KeyLit::Text(text)), None) | (_, None, None, Some(text)) => {
                    Key::Text(text)
                }
                (_, None, None, None) => Key::Text(ident.unraw().to_string()),
            };
            if let Key::Text(text) = &key {
                check_name(text, field.span())?;
            }
            Ok(Field {
                member: Member::Named(ident),
                ty: field.ty.clone(),
                key,
                omit_if_absent: match form {
                    StructForm::Map { .. } => attrs.skip_if_none,
                    StructForm::Array => is_option,
                },
            })
        })
        .collect()
}

/// A tuple struct's fields, in order: never omitted, so every one is written, as serde does.
pub fn positional(fields: &[&syn::Field]) -> syn::Result<Vec<Field>> {
    within_limit(fields.len())?;
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            bare(&field.attrs, "a tuple struct's field")?;
            Ok(Field {
                member: Member::Unnamed(Index::from(index)),
                ty: field.ty.clone(),
                key: Key::Position,
                omit_if_absent: false,
            })
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
        field
            .unraw()
            .to_string()
            .split('_')
            .flat_map(|word| {
                word.chars()
                    .take(1)
                    .flat_map(char::to_uppercase)
                    .chain(word.chars().skip(1))
            })
            .collect::<String>(),
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
