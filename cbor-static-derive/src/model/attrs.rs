//! `#[cbor(...)]` and `#[serde(...)]` attributes. `#[serde]` accepts only serde's own spellings;
//! `#[cbor]` accepts every attribute and overrides (D-08).

use proc_macro2::Span;
use syn::{Attribute, Expr, ExprLit, ExprUnary, Lit, LitInt, LitStr, Path, UnOp};

use super::Fallback;
use super::case::Case;

/// One attribute and where it was written.
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

pub enum Attr {
    Rename(String),
    RenameAll(Case),
    /// `default`, or with a path, `default = "path"`.
    Default(Option<Path>),
    SkipIfNone,
    Key(KeyLit),
    Keys(Path),
    Array,
    Index,
    CborTag(u64),
    DenyUnknownFields,
    Bytes,
    Tag(String),
    Content(String),
    Untagged,
}

pub enum KeyLit {
    Int(i64),
    Text(String),
}

#[derive(Default)]
pub struct StructAttrs {
    pub keys: Option<Path>,
    pub array: bool,
    pub deny_unknown_fields: bool,
    pub rename_all: Option<Case>,
}

#[derive(Default)]
pub struct EnumAttrs {
    pub form: FormAttrs,
    pub rename_all: Option<Case>,
}

/// The attributes that choose an enum's representation.
#[derive(Default)]
pub struct FormAttrs {
    pub index: bool,
    pub tag: Option<String>,
    pub content: Option<String>,
    pub untagged: bool,
}

#[derive(Default)]
pub struct FieldAttrs {
    pub rename: Option<String>,
    pub default: Fallback,
    pub skip_if_none: bool,
    pub key: Option<KeyLit>,
    pub bytes: bool,
}

#[derive(Default)]
pub struct VariantAttrs {
    pub rename: Option<String>,
    pub cbor_tag: Option<u64>,
}

impl StructAttrs {
    pub fn with(self, attr: Spanned<Attr>) -> syn::Result<Self> {
        match attr.value {
            Attr::Keys(keys) => Ok(Self {
                keys: Some(keys),
                ..self
            }),
            Attr::Array => Ok(Self {
                array: true,
                ..self
            }),
            Attr::DenyUnknownFields => Ok(Self {
                deny_unknown_fields: true,
                ..self
            }),
            Attr::RenameAll(case) => Ok(Self {
                rename_all: Some(case),
                ..self
            }),
            _ => Err(misplaced(attr.span, "a struct")),
        }
    }
}

impl EnumAttrs {
    pub fn with(self, attr: Spanned<Attr>) -> syn::Result<Self> {
        match attr.value {
            Attr::Index => Ok(Self {
                form: FormAttrs {
                    index: true,
                    ..self.form
                },
                ..self
            }),
            Attr::Tag(tag) => Ok(Self {
                form: FormAttrs {
                    tag: Some(tag),
                    ..self.form
                },
                ..self
            }),
            Attr::Content(content) => Ok(Self {
                form: FormAttrs {
                    content: Some(content),
                    ..self.form
                },
                ..self
            }),
            Attr::Untagged => Ok(Self {
                form: FormAttrs {
                    untagged: true,
                    ..self.form
                },
                ..self
            }),
            Attr::RenameAll(case) => Ok(Self {
                rename_all: Some(case),
                ..self
            }),
            _ => Err(misplaced(attr.span, "an enum")),
        }
    }
}

impl FieldAttrs {
    pub fn with(self, attr: Spanned<Attr>) -> syn::Result<Self> {
        match attr.value {
            Attr::Rename(rename) => Ok(Self {
                rename: Some(rename),
                ..self
            }),
            Attr::Default(path) => Ok(Self {
                default: path.map_or(Fallback::Default, Fallback::Path),
                ..self
            }),
            Attr::SkipIfNone => Ok(Self {
                skip_if_none: true,
                ..self
            }),
            Attr::Key(key) => Ok(Self {
                key: Some(key),
                ..self
            }),
            Attr::Bytes => Ok(Self {
                bytes: true,
                ..self
            }),
            _ => Err(misplaced(attr.span, "a field")),
        }
    }
}

impl VariantAttrs {
    pub fn with(self, attr: Spanned<Attr>) -> syn::Result<Self> {
        match attr.value {
            Attr::Rename(rename) => Ok(Self {
                rename: Some(rename),
                ..self
            }),
            Attr::CborTag(tag) => Ok(Self {
                cbor_tag: Some(tag),
                ..self
            }),
            _ => Err(misplaced(attr.span, "a variant")),
        }
    }
}

/// Every `#[cbor]` and `#[serde]` attribute in `attrs`, in order.
pub fn parse(attrs: &[Attribute]) -> syn::Result<Vec<Spanned<Attr>>> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cbor") || attr.path().is_ident("serde"))
        .try_fold(Vec::new(), |mut parsed, attr| {
            let cbor = attr.path().is_ident("cbor");
            attr.parse_nested_meta(|meta| {
                let span = meta
                    .path
                    .get_ident()
                    .map_or_else(Span::call_site, syn::Ident::span);
                let name = meta
                    .path
                    .get_ident()
                    .map_or_else(String::new, ToString::to_string);
                let value = match (name.as_str(), cbor) {
                    ("rename", _) => Attr::Rename(meta.value()?.parse::<LitStr>()?.value()),
                    ("rename_all", _) => {
                        let rule = meta.value()?.parse::<LitStr>()?;
                        Attr::RenameAll(Case::named(&rule.value()).ok_or_else(|| {
                            syn::Error::new(
                                rule.span(),
                                "cbor-static: `rename_all` is \"snake_case\", \"camelCase\" \
                                 or \"PascalCase\"",
                            )
                        })?)
                    }
                    ("default", _) if meta.input.peek(syn::Token![=]) => {
                        Attr::Default(Some(meta.value()?.parse::<LitStr>()?.parse::<Path>()?))
                    }
                    ("default", _) => Attr::Default(None),
                    ("skip_serializing_if", _) => {
                        if meta.value()?.parse::<LitStr>()?.value() != "Option::is_none" {
                            return Err(meta.error(
                                "cbor-static: only `skip_serializing_if = \"Option::is_none\"`",
                            ));
                        }
                        Attr::SkipIfNone
                    }
                    ("deny_unknown_fields", _) => Attr::DenyUnknownFields,
                    ("tag", _) => Attr::Tag(meta.value()?.parse::<LitStr>()?.value()),
                    ("content", _) => Attr::Content(meta.value()?.parse::<LitStr>()?.value()),
                    ("untagged", _) => Attr::Untagged,
                    ("key", true) => Attr::Key(key(&meta.value()?.parse::<Expr>()?)?),
                    ("keys", true) => Attr::Keys(meta.value()?.parse::<Path>()?),
                    ("array", true) => Attr::Array,
                    ("bytes", true) => Attr::Bytes,
                    ("index", true) => Attr::Index,
                    ("cbor_tag", true) => {
                        Attr::CborTag(meta.value()?.parse::<LitInt>()?.base10_parse()?)
                    }
                    _ => return Err(meta.error("cbor-static: unsupported attribute")),
                };
                parsed.push(Spanned { value, span });
                Ok(())
            })?;
            Ok(parsed)
        })
}

/// A field key: a string, or an integer literal that may be negated.
fn key(expr: &Expr) -> syn::Result<KeyLit> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) => Ok(KeyLit::Text(text.value())),
        Expr::Lit(ExprLit {
            lit: Lit::Int(int), ..
        }) => int.base10_parse().map(KeyLit::Int),
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => match key(expr)? {
            KeyLit::Int(int) => Ok(KeyLit::Int(-int)),
            KeyLit::Text(_) => Err(syn::Error::new_spanned(
                expr,
                "cbor-static: a key is an integer or a string",
            )),
        },
        other => Err(syn::Error::new_spanned(
            other,
            "cbor-static: a key is an integer or a string",
        )),
    }
}

fn misplaced(span: Span, position: &str) -> syn::Error {
    syn::Error::new(
        span,
        format!("cbor-static: this attribute does not apply to {position}"),
    )
}
