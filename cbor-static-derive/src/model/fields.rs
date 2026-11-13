use proc_macro2::{Span, TokenStream};
use quote::format_ident;
use syn::ext::IdentExt;
use syn::spanned::Spanned as _;
use syn::{Attribute, Fields, FieldsUnnamed, Ident, Index, Member, Path, Type};

use super::attrs::{self, FieldAttrs, KeyLit, Role, VariantAttrs};
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
    if let Payload::Struct(fields) = &payload
        && let Some(field) = fields
            .iter()
            .find(|field| matches!(field.key, Key::Flattened))
    {
        return Err(syn::Error::new(
            field.ty.span(),
            "cbor-static: `flatten` goes on a struct's fields, not a variant's",
        ));
    }
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
            borrowed(field, attrs.borrow.as_deref())?;
            match attrs.role {
                Role::Skip => return skip(field, ident, attrs),
                Role::Flatten => return flattened(field, ident, form, attrs),
                Role::Keyed => {}
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
            borrow: None,
            role: _,
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

/// A `flatten` field (#14 Q34): its type's entries join a map-form struct's own, so it has no key
/// of its own and takes no other attribute.
fn flattened(
    field: &syn::Field,
    ident: Ident,
    form: StructForm,
    attrs: FieldAttrs,
) -> syn::Result<Field> {
    match (form, attrs) {
        (StructForm::Array, _) => Err(syn::Error::new(
            field.span(),
            "cbor-static: `flatten` joins a map, so an `array` struct takes none",
        )),
        (
            StructForm::Map { .. },
            FieldAttrs {
                rename: None,
                default: Fallback::Type,
                skip_if_none: false,
                role: _,
                key: None,
                bytes: false,
                borrow: None,
            },
        ) => Ok(Field {
            member: Member::Named(ident),
            ty: field.ty.clone(),
            key: Key::Flattened,
            fallback: Fallback::Type,
            omit_if_absent: false,
            bytes: false,
        }),
        (StructForm::Map { .. }, _) => Err(syn::Error::new(
            field.span(),
            "cbor-static: a `flatten` field takes no other field attribute",
        )),
    }
}

/// A tuple or newtype struct's fields, in order: never omitted, so every one is written, as serde
/// does.
pub fn positional(fields: &FieldsUnnamed) -> syn::Result<Vec<Field>> {
    within_limit(fields.unnamed.len())?;
    fields
        .unnamed
        .iter()
        .enumerate()
        .map(|(index, field)| lone(field, Member::Unnamed(Index::from(index))))
        .collect()
}

/// A field identified by its place alone: a tuple struct's, a newtype variant's, or a transparent
/// struct's one field. It takes `#[cbor(bytes)]` and `borrow`, and nothing else.
pub fn lone(field: &syn::Field, member: Member) -> syn::Result<Field> {
    match attrs::parse(&field.attrs)?
        .into_iter()
        .try_fold(FieldAttrs::default(), FieldAttrs::with)?
    {
        FieldAttrs {
            rename: None,
            default: Fallback::Type,
            skip_if_none: false,
            role: Role::Keyed,
            key: None,
            bytes,
            borrow,
        } => borrowed(field, borrow.as_deref()).map(|()| Field {
            member,
            ty: field.ty.clone(),
            key: Key::Position,
            fallback: Fallback::Type,
            omit_if_absent: false,
            bytes,
        }),
        _ => Err(syn::Error::new(
            field.span(),
            "cbor-static: a positional field takes only `#[cbor(bytes)]` and `borrow`",
        )),
    }
}

/// `borrow` is a no-op, since a borrowing field decodes zero-copy anyway, but as in serde it needs a
/// lifetime of the field's type to borrow, and each lifetime it names must be one.
fn borrowed(field: &syn::Field, borrow: Option<&[String]>) -> syn::Result<()> {
    use quote::ToTokens as _;

    let has = lifetimes(field.ty.to_token_stream());
    match borrow {
        None => Ok(()),
        Some(_) if has.is_empty() => Err(syn::Error::new(
            field.ty.span(),
            "cbor-static: `borrow` needs a field type with a lifetime",
        )),
        Some(named) if named.iter().all(|lifetime| has.contains(lifetime)) => Ok(()),
        Some(_) => Err(syn::Error::new(
            field.ty.span(),
            "cbor-static: `borrow` names a lifetime this field's type does not have",
        )),
    }
}

/// Whether a field of type `ty` holds a value of the type `ident`: it is `ident` or `Self`, or an
/// array, a tuple or a container of the vocabulary that holds one. A type the derive does not know,
/// such as `Id<Node>` or `PhantomData<Self>`, may name the type without holding it.
pub fn holds(ty: &Type, ident: &Ident) -> bool {
    use syn::{GenericArgument, PathArguments};

    const HOLDERS: [&str; 8] = [
        "Option",
        "Tagged",
        "SelfDescribed",
        "ArrayRef",
        "MapRef",
        "Vec",
        "LinearMap",
        "Box",
    ];
    match ty {
        Type::Path(path) if path.qself.is_none() && path.path.is_ident("Self") => true,
        Type::Path(path) if path.qself.is_none() => path.path.segments.last().is_some_and(|last| {
            (path.path.segments.len() == 1 && last.ident == *ident)
                || (HOLDERS.contains(&last.ident.to_string().as_str())
                    && matches!(&last.arguments, PathArguments::AngleBracketed(arguments)
                    if arguments.args.iter().any(|argument| {
                        matches!(argument, GenericArgument::Type(inner) if holds(inner, ident))
                    })))
        }),
        Type::Array(array) => holds(&array.elem, ident),
        Type::Tuple(tuple) => tuple.elems.iter().any(|elem| holds(elem, ident)),
        Type::Paren(inner) => holds(&inner.elem, ident),
        Type::Group(inner) => holds(&inner.elem, ident),
        _ => false,
    }
}

/// The lifetimes in `tokens` other than `'static`, as written: `'a`.
fn lifetimes(tokens: TokenStream) -> Vec<String> {
    use proc_macro2::TokenTree;

    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    trees
        .iter()
        .enumerate()
        .flat_map(|(at, tree)| {
            match tree {
            TokenTree::Group(group) => lifetimes(group.stream()),
            TokenTree::Ident(name)
                if name != "static"
                    && at.checked_sub(1).and_then(|before| trees.get(before)).is_some_and(
                        |before| matches!(before, TokenTree::Punct(tick) if tick.as_char() == '\''),
                    ) =>
            {
                vec![format!("'{name}")]
            }
            _ => vec![],
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

/// The `keys = Label` variant a field maps to: its name in `UpperCamelCase`, by Rust's naming
/// rules, which change case for any letter, where serde's `rename_all` (`Case`) changes only ASCII.
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

#[cfg(test)]
mod tests {
    use quote::format_ident;
    use syn::{Type, parse_quote};

    use super::holds;

    #[test]
    fn a_field_holds_its_type_directly_or_through_the_vocabulary() {
        let node = format_ident!("Node");
        let holding: [Type; 7] = [
            parse_quote!(Node),
            parse_quote!(Self),
            parse_quote!(Option<Self>),
            parse_quote!([Node; 2]),
            parse_quote!((u32, Node)),
            parse_quote!(cbor_static::ArrayRef<'a, Node<'a>>),
            parse_quote!(heapless::Vec<Option<Node>, 4>),
        ];
        for ty in holding {
            assert!(holds(&ty, &node), "{ty}", ty = quote::quote!(#ty));
        }
        let naming: [Type; 4] = [
            parse_quote!(Id<Node>),
            parse_quote!(core::marker::PhantomData<Self>),
            parse_quote!(other::Node),
            parse_quote!(u32),
        ];
        for ty in naming {
            assert!(!holds(&ty, &node), "{ty}", ty = quote::quote!(#ty));
        }
    }
}
