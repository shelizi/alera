use tree_sitter::Language;

pub(crate) struct NativeLanguageDescriptor {
    pub canonical_id: &'static str,
    pub aliases: &'static [&'static str],
    pub language: Language,
    pub highlight_query: String,
}

pub(crate) struct NativeLanguageResolution {
    pub canonical_id: String,
    pub descriptor: Option<NativeLanguageDescriptor>,
}

struct NativeLanguageRegistration {
    canonical_id: &'static str,
    aliases: &'static [&'static str],
    build: fn() -> (Language, String),
}

const NATIVE_LANGUAGE_REGISTRY: &[NativeLanguageRegistration] = &[
    NativeLanguageRegistration {
        canonical_id: "csharp",
        aliases: &["cs"],
        build: csharp_language,
    },
    NativeLanguageRegistration {
        canonical_id: "dart",
        aliases: &[],
        build: dart_language,
    },
    NativeLanguageRegistration {
        canonical_id: "go",
        aliases: &[],
        build: go_language,
    },
    NativeLanguageRegistration {
        canonical_id: "rust",
        aliases: &["rs"],
        build: rust_language,
    },
    NativeLanguageRegistration {
        canonical_id: "javascript",
        aliases: &["js", "jsx"],
        build: javascript_language,
    },
    NativeLanguageRegistration {
        canonical_id: "typescript",
        aliases: &["ts"],
        build: typescript_language,
    },
    NativeLanguageRegistration {
        canonical_id: "tsx",
        aliases: &[],
        build: tsx_language,
    },
    NativeLanguageRegistration {
        canonical_id: "python",
        aliases: &["py"],
        build: python_language,
    },
    NativeLanguageRegistration {
        canonical_id: "php",
        aliases: &[],
        build: php_language,
    },
    NativeLanguageRegistration {
        canonical_id: "json",
        aliases: &["jsonc"],
        build: json_language,
    },
];

pub(crate) fn resolve_native_language(language_id: &str) -> NativeLanguageResolution {
    let normalized = language_id.trim().to_lowercase();
    let registration = NATIVE_LANGUAGE_REGISTRY.iter().find(|registration| {
        registration.canonical_id == normalized
            || registration
                .aliases
                .iter()
                .any(|alias| *alias == normalized)
    });

    let Some(registration) = registration else {
        return NativeLanguageResolution {
            canonical_id: normalized,
            descriptor: None,
        };
    };

    let (language, highlight_query) = (registration.build)();
    NativeLanguageResolution {
        canonical_id: registration.canonical_id.to_string(),
        descriptor: Some(NativeLanguageDescriptor {
            canonical_id: registration.canonical_id,
            aliases: registration.aliases,
            language,
            highlight_query,
        }),
    }
}

fn csharp_language() -> (Language, String) {
    (
        tree_sitter_c_sharp::LANGUAGE.into(),
        tree_sitter_c_sharp::HIGHLIGHTS_QUERY.to_string(),
    )
}

fn dart_language() -> (Language, String) {
    (
        tree_sitter_dart::LANGUAGE.into(),
        tree_sitter_dart::HIGHLIGHTS_QUERY.to_string(),
    )
}

fn go_language() -> (Language, String) {
    (
        tree_sitter_go::LANGUAGE.into(),
        tree_sitter_go::HIGHLIGHTS_QUERY.to_string(),
    )
}

fn rust_language() -> (Language, String) {
    (
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
    )
}

fn javascript_language() -> (Language, String) {
    (
        tree_sitter_javascript::LANGUAGE.into(),
        format!(
            "{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
        ),
    )
}

fn typescript_language() -> (Language, String) {
    (
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        format!(
            "{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY
        ),
    )
}

fn tsx_language() -> (Language, String) {
    (
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        format!(
            "{}\n{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY
        ),
    )
}

fn python_language() -> (Language, String) {
    (
        tree_sitter_python::LANGUAGE.into(),
        tree_sitter_python::HIGHLIGHTS_QUERY.to_string(),
    )
}

fn php_language() -> (Language, String) {
    (
        tree_sitter_php::LANGUAGE_PHP.into(),
        tree_sitter_php::HIGHLIGHTS_QUERY.to_string(),
    )
}

fn json_language() -> (Language, String) {
    (
        tree_sitter_json::LANGUAGE.into(),
        tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tree_sitter::{Parser, Query};

    #[test]
    fn native_language_aliases_resolve_to_canonical_ids() {
        for (alias, canonical) in [
            ("JS", "javascript"),
            ("jsx", "javascript"),
            ("ts", "typescript"),
            ("py", "python"),
            ("rs", "rust"),
            ("jsonc", "json"),
        ] {
            let resolved = resolve_native_language(alias);
            assert_eq!(resolved.canonical_id, canonical);
            let descriptor = resolved.descriptor.expect("alias must resolve");
            assert_eq!(descriptor.canonical_id, canonical);
            let normalized_alias = alias.to_lowercase();
            assert!(descriptor.aliases.contains(&normalized_alias.as_str()));
            assert!(!descriptor.highlight_query.is_empty());
        }
    }

    #[test]
    fn unsupported_language_keeps_normalized_metadata_without_a_parser() {
        let resolved = resolve_native_language("  Kotlin  ");
        assert_eq!(resolved.canonical_id, "kotlin");
        assert!(resolved.descriptor.is_none());
    }

    #[test]
    fn native_language_registry_has_unique_ids_aliases_and_queries() {
        let mut keys = HashSet::new();
        for registration in NATIVE_LANGUAGE_REGISTRY {
            assert!(keys.insert(registration.canonical_id));
            for alias in registration.aliases {
                assert!(keys.insert(*alias));
            }
            let resolved = resolve_native_language(registration.canonical_id);
            let descriptor = resolved
                .descriptor
                .expect("registered language must resolve");
            assert_eq!(descriptor.canonical_id, registration.canonical_id);
            assert_eq!(descriptor.aliases, registration.aliases);
            assert!(!descriptor.highlight_query.is_empty());
        }
    }

    #[test]
    fn first_wave_expansion_grammars_parse_representative_source() {
        for (language_id, canonical_id, source) in [
            (
                "cs",
                "csharp",
                "namespace Demo; class Greeter { string Hello() => \"hi\"; }",
            ),
            (
                "go",
                "go",
                "package demo\nfunc add(a int, b int) int { return a + b }",
            ),
            (
                "php",
                "php",
                "<?php class Greeter { public function hello(): string { return 'hi'; } }",
            ),
        ] {
            let resolved = resolve_native_language(language_id);
            assert_eq!(resolved.canonical_id, canonical_id);
            let descriptor = resolved
                .descriptor
                .expect("first-wave structural grammar must resolve");
            assert!(!descriptor.highlight_query.is_empty());
            Query::new(&descriptor.language, &descriptor.highlight_query)
                .expect("registered highlight query must compile");

            let mut parser = Parser::new();
            parser
                .set_language(&descriptor.language)
                .expect("registered grammar must configure the parser");
            let tree = parser
                .parse(source, None)
                .expect("representative source must parse");
            assert!(
                !tree.root_node().has_error(),
                "{canonical_id} representative source must parse without errors"
            );
        }
    }
}
