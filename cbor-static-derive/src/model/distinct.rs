//! Two of one map's fields keyed alike, or two of one enum's variants named alike, are a compile
//! error naming both; else the later one would decode as the earlier, or as a duplicate key.

use syn::spanned::Spanned as _;

use super::{Field, Key, Variant};

/// The first two of `items` that `alike` pairs, in their order.
pub fn clash<T>(items: &[T], alike: impl Fn(&T, &T) -> bool) -> Option<(&T, &T)> {
    items.iter().enumerate().find_map(|(at, first)| {
        items
            .iter()
            .skip(at + 1)
            .find(|again| alike(first, again))
            .map(|again| (first, again))
    })
}

pub fn keys(fields: &[Field]) -> syn::Result<()> {
    clash(fields, |first, again| {
        shown(&first.key).is_some() && shown(&first.key) == shown(&again.key)
    })
    .map_or(Ok(()), |(first, again)| {
        Err(syn::Error::new(
            again.member.span(),
            format!(
                "cbor-static: fields `{first}` and `{again}` share the key {key}",
                first = member(first),
                again = member(again),
                key = shown(&again.key).unwrap_or_default(),
            ),
        ))
    })
}

pub fn names(variants: &[Variant]) -> syn::Result<()> {
    clash(variants, |first, again| first.name == again.name).map_or(Ok(()), |(first, again)| {
        Err(syn::Error::new(
            again.ident.span(),
            format!(
                "cbor-static: variants `{first}` and `{again}` share the name {name:?}",
                first = first.ident,
                again = again.ident,
                name = again.name,
            ),
        ))
    })
}

/// A key as the error shows it, alike exactly when the keys are; `None` for a field no key names.
fn shown(key: &Key) -> Option<String> {
    match key {
        Key::Text(text) => Some(format!("{text:?}")),
        Key::Int(int) => Some(int.to_string()),
        Key::Label { label, variant } => Some(format!(
            "{label}::{variant}",
            label = label
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
        )),
        Key::Position | Key::Skipped | Key::Flattened | Key::Other => None,
    }
}

fn member(field: &Field) -> String {
    match &field.member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(index) => index.index.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, clash, shown};

    #[test]
    fn clash_finds_the_first_pair_in_order() {
        assert_eq!(clash(&[1, 2, 3, 2, 1], |a, b| a == b), Some((&1, &1)));
        assert_eq!(clash(&[1, 2, 3], |a, b| a == b), None);
        assert_eq!(clash::<u8>(&[], |a, b| a == b), None);
    }

    #[test]
    fn keys_are_shown_alike_exactly_when_they_are_alike() {
        let label = |variant: &str| Key::Label {
            label: syn::parse_quote!(crate::Label),
            variant: syn::Ident::new(variant, proc_macro2::Span::call_site()),
        };
        assert_eq!(shown(&Key::Text("a-b".into())).as_deref(), Some("\"a-b\""));
        assert_eq!(shown(&Key::Int(-4)).as_deref(), Some("-4"));
        assert_eq!(
            shown(&label("KeyOps")).as_deref(),
            Some("crate::Label::KeyOps")
        );
        assert_ne!(shown(&Key::Text("4".into())), shown(&Key::Int(4)));
        assert_ne!(shown(&label("A")), shown(&label("B")));
        for unnamed in [Key::Position, Key::Skipped, Key::Flattened, Key::Other] {
            assert_eq!(shown(&unnamed), None);
        }
    }
}
