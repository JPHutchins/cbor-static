//! An enum's representation (D-10), its variants' CBOR tags, and whether its layout comes from
//! primitive-`repr` tables or from glue (D-21).

use proc_macro2::Span;
use syn::Attribute;
use syn::spanned::Spanned as _;

use super::attrs::{self, EnumAttrs, FormAttrs};
use super::case::Case;
use super::fields::{check_name, variant};
use super::{EnumForm, Key, Offsets, Payload, Repr, Shape, Variant};

pub fn enumeration(item_attrs: &[Attribute], data: &syn::DataEnum) -> syn::Result<Shape> {
    let attrs = attrs::parse(item_attrs)?
        .into_iter()
        .try_fold(EnumAttrs::default(), EnumAttrs::with)?;
    let variants = data
        .variants
        .iter()
        .map(|each| variant(each, attrs.rename_all))
        .collect::<syn::Result<Vec<_>>>()?;
    let form = form(attrs.form, &variants)?;
    names_written(&form, attrs.rename_all, &variants)?;
    if let EnumForm::Internal { tag } | EnumForm::Adjacent { tag, .. } = &form {
        check_name(tag, Span::call_site())?;
    }
    if let EnumForm::Adjacent { content, .. } = &form {
        check_name(content, Span::call_site())?;
    }
    cbor_tags(&form, &variants)?;
    tag_keys(&form, &variants)?;
    let repr = tables_or_glue(repr(item_attrs)?, &variants);
    Ok(Shape::Enum {
        offsets: offsets(attrs.offset_of_enum, repr)?,
        repr,
        variants,
        form,
    })
}

/// The one representation the enum's attributes and its variants' `cbor_tag`s choose.
fn form(attrs: FormAttrs, variants: &[Variant]) -> syn::Result<EnumForm> {
    match (
        attrs,
        variants.iter().filter(|v| v.cbor_tag.is_some()).count(),
    ) {
        (
            FormAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: false,
            },
            0,
        ) => Ok(EnumForm::External),
        (
            FormAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: false,
            },
            all,
        ) if all == variants.len() => Ok(EnumForm::CborTag),
        (
            FormAttrs {
                index: true,
                tag: None,
                content: None,
                untagged: false,
            },
            0,
        ) => Ok(EnumForm::Index),
        (
            FormAttrs {
                index: false,
                tag: Some(tag),
                content: None,
                untagged: false,
            },
            0,
        ) => Ok(EnumForm::Internal { tag }),
        (
            FormAttrs {
                index: false,
                tag: Some(tag),
                content: Some(content),
                untagged: false,
            },
            0,
        ) => Ok(EnumForm::Adjacent { tag, content }),
        (
            FormAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: true,
            },
            0,
        ) => Ok(EnumForm::Untagged),
        (
            FormAttrs {
                index: false,
                tag: None,
                content: None,
                untagged: false,
            },
            _,
        ) => Err(syn::Error::new(
            variants
                .iter()
                .find(|v| v.cbor_tag.is_none())
                .map_or_else(Span::call_site, |untagged| untagged.ident.span()),
            "cbor-static: `cbor_tag` goes on every variant or on none",
        )),
        _ => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: one enum representation: `index`, `tag` (with `content`), \
             `untagged`, or `cbor_tag` on every variant",
        )),
    }
}

/// Only the forms that write variant names take `rename_all` or a variant's `rename` (Q8).
fn names_written(
    form: &EnumForm,
    rename_all: Option<Case>,
    variants: &[Variant],
) -> syn::Result<()> {
    match (
        form,
        rename_all,
        variants.iter().find(|variant| variant.renamed),
    ) {
        (EnumForm::External | EnumForm::Internal { .. } | EnumForm::Adjacent { .. }, ..)
        | (_, None, None) => Ok(()),
        (EnumForm::Index | EnumForm::CborTag | EnumForm::Untagged, _, renamed) => {
            Err(syn::Error::new(
                renamed.map_or_else(Span::call_site, |variant| variant.ident.span()),
                "cbor-static: an `index`, `cbor_tag` or untagged enum writes no variant names, \
                 so it takes neither `rename_all` nor `rename`",
            ))
        }
    }
}

/// A `cbor_tag` enum's tags are distinct, and every variant has a payload to tag.
fn cbor_tags(form: &EnumForm, variants: &[Variant]) -> syn::Result<()> {
    if let Some((first, again)) = variants.iter().enumerate().find_map(|(at, first)| {
        variants
            .iter()
            .skip(at + 1)
            .find(|again| again.cbor_tag.is_some() && again.cbor_tag == first.cbor_tag)
            .map(|again| (first, again))
    }) {
        return Err(syn::Error::new(
            again.ident.span(),
            format!(
                "cbor-static: variants `{}` and `{}` share a `cbor_tag`",
                first.ident, again.ident
            ),
        ));
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
    Ok(())
}

/// The tag key shares its map with an internally tagged struct variant's fields, and with an
/// adjacently tagged enum's content key, so neither may reuse it.
fn tag_keys(form: &EnumForm, variants: &[Variant]) -> syn::Result<()> {
    match form {
        EnumForm::Internal { tag } => variants
            .iter()
            .filter_map(|variant| match &variant.payload {
                Payload::Struct(fields) => Some(fields),
                Payload::Unit | Payload::Newtype(_) => None,
            })
            .flatten()
            .find(|field| matches!(&field.key, Key::Text(key) if key == tag))
            .map_or(Ok(()), |field| {
                Err(syn::Error::new(
                    field.member.span(),
                    format!("cbor-static: this field's key is the enum's tag, `{tag}`"),
                ))
            }),
        EnumForm::Adjacent { tag, content } if tag == content => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: `tag` and `content` must differ",
        )),
        EnumForm::External
        | EnumForm::Index
        | EnumForm::CborTag
        | EnumForm::Adjacent { .. }
        | EnumForm::Untagged => Ok(()),
    }
}

/// `#[cbor(offset_of_enum)]` (R-129) takes the tables' payload offsets from the enum itself, so it
/// needs tables: a primitive `repr` and implicit discriminants.
fn offsets(offset_of_enum: bool, repr: Repr) -> syn::Result<Offsets> {
    match (offset_of_enum, repr) {
        (false, _) => Ok(Offsets::Shadows),
        (true, Repr::U8 | Repr::CU8) => Ok(Offsets::Enum),
        (true, Repr::Rust) => Err(syn::Error::new(
            Span::call_site(),
            "cbor-static: `offset_of_enum` needs repr(u8) or repr(C, u8) and implicit \
             discriminants",
        )),
    }
}

/// A primitive-`repr` enum's tables write and read each variant's position as its tag byte, which
/// holds only while every discriminant is implicit; any other enum takes the glue.
fn tables_or_glue(repr: Repr, variants: &[Variant]) -> Repr {
    if variants
        .iter()
        .any(|variant| variant.discriminant.is_some())
    {
        Repr::Rust
    } else {
        repr
    }
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
