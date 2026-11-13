//! `rename_all`'s rules (#13 Q30), exactly as serde applies them: `serde_derive`'s `RenameRule`
//! (`apply_to_field`, `apply_to_variant`) at 1.0.229, whose own test tables the tests below repeat.

/// A `rename_all` rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Snake,
    Camel,
    Pascal,
}

/// The rules cbor-static accepts, by the name `rename_all = "…"` gives each.
pub const RULES: [(&str, Case); 3] = [
    ("snake_case", Case::Snake),
    ("camelCase", Case::Camel),
    ("PascalCase", Case::Pascal),
];

impl Case {
    /// The rule `rename_all = "…"` names, if it is one of [`RULES`].
    pub fn named(rule: &str) -> Option<Self> {
        RULES
            .iter()
            .find(|(name, _)| *name == rule)
            .map(|&(_, case)| case)
    }

    /// A field's name, written in `snake_case`, under this rule; `Snake` keeps any name as written.
    pub fn field(self, field: &str) -> String {
        match self {
            Self::Snake => field.to_owned(),
            Self::Pascal => field
                .split('_')
                .flat_map(|word| {
                    word.chars()
                        .take(1)
                        .map(|first| first.to_ascii_uppercase())
                        .chain(word.chars().skip(1))
                })
                .collect(),
            Self::Camel => lower_first(&Self::Pascal.field(field)),
        }
    }

    /// A variant's name, written in `PascalCase`, under this rule; `Pascal` keeps any name as
    /// written.
    pub fn variant(self, variant: &str) -> String {
        match self {
            Self::Pascal => variant.to_owned(),
            Self::Camel => lower_first(variant),
            Self::Snake => variant
                .char_indices()
                .flat_map(|(at, ch)| {
                    (at > 0 && ch.is_uppercase())
                        .then_some('_')
                        .into_iter()
                        .chain([ch.to_ascii_lowercase()])
                })
                .collect(),
        }
    }
}

fn lower_first(name: &str) -> String {
    name.chars()
        .take(1)
        .map(|first| first.to_ascii_lowercase())
        .chain(name.chars().skip(1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Case, RULES};

    #[test]
    fn variants_follow_serde() {
        for (original, camel, snake) in [
            ("Outcome", "outcome", "outcome"),
            ("VeryTasty", "veryTasty", "very_tasty"),
            ("A", "a", "a"),
            ("Z42", "z42", "z42"),
        ] {
            assert_eq!(Case::Pascal.variant(original), original);
            assert_eq!(Case::Camel.variant(original), camel);
            assert_eq!(Case::Snake.variant(original), snake);
        }
    }

    #[test]
    fn fields_follow_serde() {
        for (original, pascal, camel) in [
            ("outcome", "Outcome", "outcome"),
            ("very_tasty", "VeryTasty", "veryTasty"),
            ("a", "A", "a"),
            ("z42", "Z42", "z42"),
        ] {
            assert_eq!(Case::Snake.field(original), original);
            assert_eq!(Case::Pascal.field(original), pascal);
            assert_eq!(Case::Camel.field(original), camel);
        }
    }

    #[test]
    fn the_default_rules_keep_any_name_as_written() {
        for name in ["key_ops", "KeyOps", "keyOps", "r#type"] {
            assert_eq!(Case::Snake.field(name), name);
            assert_eq!(Case::Pascal.variant(name), name);
        }
    }

    #[test]
    fn only_the_listed_rules_are_accepted() {
        for (name, case) in RULES {
            assert_eq!(Case::named(name), Some(case), "{name}");
        }
        for rejected in [
            "lowercase",
            "UPPERCASE",
            "SCREAMING_SNAKE_CASE",
            "kebab-case",
            "",
        ] {
            assert_eq!(Case::named(rejected), None, "{rejected}");
        }
    }
}
