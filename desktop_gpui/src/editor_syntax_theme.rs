//! Generated from Classic editor_syntax_theme_catalog.dart and re_highlight 0.0.3 colors.
//! Regenerate with python desktop_gpui/tool/generate_editor_syntax_palettes.py.
//! Colors describe Classic tokens; Syntect remains the GPUI syntax parser.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorSyntaxPalette {
    pub name: &'static str,
    pub background: u32,
    pub foreground: u32,
    pub tokens: &'static [(&'static str, u32)],
    /// Optional per-token backgrounds (e.g. Classic diff addition/deletion).
    pub backgrounds: &'static [(&'static str, u32)],
    /// Classic TextStyle weights: bit 0 bold, bit 1 italic.
    pub modifiers: &'static [(&'static str, u8)],
}

impl EditorSyntaxPalette {
    pub fn color(&self, category: &str) -> Option<u32> {
        if category == "root" {
            return Some(self.foreground);
        }
        self.tokens
            .iter()
            .find(|(name, _)| *name == category)
            .map(|(_, color)| *color)
    }
    pub fn first_color(&self, categories: &[&str]) -> Option<u32> {
        categories.iter().find_map(|name| self.color(name))
    }
    pub fn background(&self, category: &str) -> Option<u32> {
        self.backgrounds
            .iter()
            .find(|(name, _)| *name == category)
            .map(|(_, color)| *color)
    }
    pub fn first_background(&self, categories: &[&str]) -> Option<u32> {
        categories.iter().find_map(|name| self.background(name))
    }
    pub fn modifier(&self, category: &str) -> Option<u8> {
        self.modifiers
            .iter()
            .find(|(name, _)| *name == category)
            .map(|(_, bits)| *bits)
    }
    pub fn first_modifier(&self, categories: &[&str]) -> Option<u8> {
        categories.iter().find_map(|name| self.modifier(name))
    }
}

pub const CLASSIC_EDITOR_SYNTAX_PALETTES: &[EditorSyntaxPalette] = &[
    EditorSyntaxPalette {
        name: "Alera",
        background: 0x101010,
        foreground: 0xf5f5f5,
        tokens: &[
            ("keyword", 0xc792ea),
            ("meta-keyword", 0xc792ea),
            ("template-tag", 0xc792ea),
            ("operator", 0x89ddff),
            ("function", 0x82aaff),
            ("title", 0x82aaff),
            ("title.function_", 0x82aaff),
            ("string", 0x22c55e),
            ("meta-string", 0x22c55e),
            ("comment", 0x606060),
            ("quote", 0x606060),
            ("number", 0xf59e0b),
            ("literal", 0xffcb6b),
            ("built_in", 0xffcb6b),
            ("variable", 0xffcc80),
            ("type", 0x89ddff),
            ("title.class_", 0xffcb6b),
            ("class-title", 0xffcb6b),
            ("attr", 0xffcb6b),
            ("attribute", 0xffcb6b),
            ("meta", 0xa1a1a1),
            ("selector-tag", 0xc792ea),
            ("selector-class", 0xffcb6b),
            ("selector-attr", 0xffcb6b),
            ("selector-pseudo", 0x89ddff),
            ("name", 0x82aaff),
            ("subst", 0xf5f5f5),
            ("addition", 0x22c55e),
            ("deletion", 0xf87171),
        ],
        backgrounds: &[],
        modifiers: &[],
    },
    EditorSyntaxPalette {
        name: "GitHub Dark",
        background: 0x0d1117,
        foreground: 0xc9d1d9,
        tokens: &[
            ("keyword", 0xff7b72),
            ("meta-keyword", 0xff7b72),
            ("template-tag", 0xff7b72),
            ("operator", 0x79c0ff),
            ("title", 0xd2a8ff),
            ("title.function_", 0xd2a8ff),
            ("string", 0xa5d6ff),
            ("meta-string", 0xa5d6ff),
            ("comment", 0x8b949e),
            ("quote", 0x7ee787),
            ("number", 0x79c0ff),
            ("literal", 0x79c0ff),
            ("built_in", 0xffa657),
            ("variable", 0x79c0ff),
            ("type", 0xff7b72),
            ("title.class_", 0xd2a8ff),
            ("attr", 0x79c0ff),
            ("attribute", 0x79c0ff),
            ("meta", 0x79c0ff),
            ("selector-tag", 0x7ee787),
            ("selector-class", 0x79c0ff),
            ("selector-attr", 0x79c0ff),
            ("selector-pseudo", 0x7ee787),
            ("name", 0x7ee787),
            ("subst", 0xc9d1d9),
            ("addition", 0xaff5b4),
            ("deletion", 0xffdcd7),
            ("regexp", 0xa5d6ff),
            ("section", 0x1f6feb),
            ("doctag", 0xff7b72),
            ("strong", 0xc9d1d9),
            ("emphasis", 0xc9d1d9),
            ("code", 0x8b949e),
            ("symbol", 0xffa657),
            ("selector-id", 0x79c0ff),
            ("bullet", 0xf2cc60),
        ],
        backgrounds: &[("addition", 0x033a16), ("deletion", 0x67060c)],
        modifiers: &[("section", 1), ("strong", 1), ("emphasis", 2)],
    },
    EditorSyntaxPalette {
        name: "GitHub Dark Dimmed",
        background: 0x22272e,
        foreground: 0xadbac7,
        tokens: &[
            ("keyword", 0xf47067),
            ("meta-keyword", 0xf47067),
            ("template-tag", 0xf47067),
            ("operator", 0x6cb6ff),
            ("title", 0xdcbdfb),
            ("title.function_", 0xdcbdfb),
            ("string", 0x96d0ff),
            ("meta-string", 0x96d0ff),
            ("comment", 0x768390),
            ("quote", 0x8ddb8c),
            ("number", 0x6cb6ff),
            ("literal", 0x6cb6ff),
            ("built_in", 0xf69d50),
            ("variable", 0x6cb6ff),
            ("type", 0xf47067),
            ("title.class_", 0xdcbdfb),
            ("attr", 0x6cb6ff),
            ("attribute", 0x6cb6ff),
            ("meta", 0x6cb6ff),
            ("selector-tag", 0x8ddb8c),
            ("selector-class", 0x6cb6ff),
            ("selector-attr", 0x6cb6ff),
            ("selector-pseudo", 0x8ddb8c),
            ("name", 0x8ddb8c),
            ("subst", 0xadbac7),
            ("addition", 0xb4f1b4),
            ("deletion", 0xffd8d3),
            ("regexp", 0x96d0ff),
            ("section", 0x316dca),
            ("doctag", 0xf47067),
            ("strong", 0xadbac7),
            ("emphasis", 0xadbac7),
            ("code", 0x768390),
            ("symbol", 0xf69d50),
            ("selector-id", 0x6cb6ff),
            ("bullet", 0xeac55f),
        ],
        backgrounds: &[("addition", 0x1b4721), ("deletion", 0x78191b)],
        modifiers: &[("section", 1), ("strong", 1), ("emphasis", 2)],
    },
    EditorSyntaxPalette {
        name: "VS2015",
        background: 0x1e1e1e,
        foreground: 0xdcdcdc,
        tokens: &[
            ("keyword", 0x569cd6),
            ("meta-keyword", 0x9b9b9b),
            ("template-tag", 0x9a5334),
            ("function", 0xdcdcdc),
            ("title", 0xdcdcdc),
            ("string", 0xd69d85),
            ("meta-string", 0xd69d85),
            ("comment", 0x57a64a),
            ("quote", 0x57a64a),
            ("number", 0xb8d7a3),
            ("literal", 0x569cd6),
            ("built_in", 0x4ec9b0),
            ("variable", 0xbd63c5),
            ("params", 0xdcdcdc),
            ("type", 0x4ec9b0),
            ("attr", 0x9cdcfe),
            ("attribute", 0x9cdcfe),
            ("meta", 0x9b9b9b),
            ("selector-tag", 0xd7ba7d),
            ("selector-class", 0xd7ba7d),
            ("selector-attr", 0xd7ba7d),
            ("selector-pseudo", 0xd7ba7d),
            ("name", 0x569cd6),
            ("subst", 0xdcdcdc),
            ("tag", 0x9b9b9b),
            ("regexp", 0x9a5334),
            ("section", 0xffd700),
            ("doctag", 0x608b4e),
            ("symbol", 0x569cd6),
            ("selector-id", 0xd7ba7d),
            ("bullet", 0xd7ba7d),
        ],
        backgrounds: &[("addition", 0x144212), ("deletion", 0x660000)],
        modifiers: &[("comment", 2), ("quote", 2), ("strong", 1), ("emphasis", 2)],
    },
    EditorSyntaxPalette {
        name: "Atom One Dark",
        background: 0x282c34,
        foreground: 0xabb2bf,
        tokens: &[
            ("keyword", 0xc678dd),
            ("title", 0x61aeee),
            ("string", 0x98c379),
            ("meta-string", 0x98c379),
            ("comment", 0x5c6370),
            ("quote", 0x5c6370),
            ("number", 0xd19a66),
            ("literal", 0x56b6c2),
            ("built_in", 0xe6c07b),
            ("variable", 0xd19a66),
            ("type", 0xd19a66),
            ("title.class_", 0xe6c07b),
            ("class-title", 0xe6c07b),
            ("attr", 0xd19a66),
            ("attribute", 0x98c379),
            ("meta", 0x61aeee),
            ("selector-tag", 0xe06c75),
            ("selector-class", 0xd19a66),
            ("selector-attr", 0xd19a66),
            ("selector-pseudo", 0xd19a66),
            ("name", 0xe06c75),
            ("subst", 0xe06c75),
            ("addition", 0x98c379),
            ("deletion", 0xe06c75),
            ("regexp", 0x98c379),
            ("section", 0xe06c75),
            ("doctag", 0xc678dd),
            ("symbol", 0x61aeee),
            ("selector-id", 0x61aeee),
            ("bullet", 0x61aeee),
        ],
        backgrounds: &[],
        modifiers: &[("comment", 2), ("quote", 2), ("strong", 1), ("emphasis", 2)],
    },
    EditorSyntaxPalette {
        name: "Night Owl",
        background: 0x011627,
        foreground: 0xd6deeb,
        tokens: &[
            ("keyword", 0xc792ea),
            ("meta-keyword", 0x82aaff),
            ("template-tag", 0xc792ea),
            ("function", 0x82aaff),
            ("title", 0xdcdcaa),
            ("string", 0xecc48d),
            ("meta-string", 0xecc48d),
            ("comment", 0x637777),
            ("quote", 0x697098),
            ("number", 0xf78c6c),
            ("literal", 0xff5874),
            ("built_in", 0xaddb67),
            ("variable", 0xaddb67),
            ("params", 0x7fdbca),
            ("type", 0x82aaff),
            ("attr", 0x7fdbca),
            ("attribute", 0x80cbc4),
            ("meta", 0x82aaff),
            ("selector-tag", 0xff6363),
            ("selector-class", 0xaddb67),
            ("selector-attr", 0xc792ea),
            ("selector-pseudo", 0xc792ea),
            ("name", 0x7fdbca),
            ("subst", 0xd3423e),
            ("tag", 0x7fdbca),
            ("regexp", 0x5ca7e4),
            ("section", 0x82b1ff),
            ("doctag", 0x7fdbca),
            ("strong", 0xaddb67),
            ("emphasis", 0xc792ea),
            ("code", 0x80cbc4),
            ("symbol", 0x82aaff),
            ("selector-id", 0xfad430),
            ("bullet", 0xd9f5dd),
        ],
        backgrounds: &[],
        modifiers: &[
            ("keyword", 2),
            ("title", 2),
            ("comment", 2),
            ("quote", 2),
            ("built_in", 2),
            ("selector-class", 2),
            ("selector-attr", 2),
            ("selector-pseudo", 2),
            ("addition", 2),
            ("deletion", 2),
            ("strong", 1),
            ("emphasis", 2),
        ],
    },
    EditorSyntaxPalette {
        name: "Nord",
        background: 0x2e3440,
        foreground: 0xd8dee9,
        tokens: &[
            ("keyword", 0x81a1c1),
            ("meta-keyword", 0x5e81ac),
            ("template-tag", 0x5e81ac),
            ("function", 0x88c0d0),
            ("title", 0x8fbcbb),
            ("string", 0xa3be8c),
            ("meta-string", 0xa3be8c),
            ("comment", 0x4c566a),
            ("quote", 0x4c566a),
            ("number", 0xb48ead),
            ("literal", 0x81a1c1),
            ("built_in", 0x8fbcbb),
            ("variable", 0xd8dee9),
            ("params", 0xd8dee9),
            ("type", 0x8fbcbb),
            ("attr", 0x8fbcbb),
            ("attribute", 0xd8dee9),
            ("meta", 0x5e81ac),
            ("selector-tag", 0x81a1c1),
            ("selector-class", 0x8fbcbb),
            ("selector-attr", 0x8fbcbb),
            ("selector-pseudo", 0x88c0d0),
            ("name", 0x81a1c1),
            ("subst", 0xd8dee9),
            ("tag", 0x81a1c1),
            ("regexp", 0xebcb8b),
            ("property", 0x88c0d0),
            ("section", 0x88c0d0),
            ("doctag", 0x8fbcbb),
            ("code", 0x8fbcbb),
            ("symbol", 0x81a1c1),
            ("selector-id", 0x8fbcbb),
            ("bullet", 0x81a1c1),
        ],
        backgrounds: &[],
        modifiers: &[("strong", 1), ("emphasis", 2), ("selector-id", 1)],
    },
    EditorSyntaxPalette {
        name: "Monokai",
        background: 0x272822,
        foreground: 0xdddddd,
        tokens: &[
            ("keyword", 0xf92672),
            ("template-tag", 0xa6e22e),
            ("title", 0xa6e22e),
            ("string", 0xa6e22e),
            ("comment", 0x75715e),
            ("quote", 0x75715e),
            ("literal", 0xf92672),
            ("built_in", 0xa6e22e),
            ("variable", 0xa6e22e),
            ("type", 0xa6e22e),
            ("title.class_", 0xffffff),
            ("class-title", 0xffffff),
            ("attribute", 0xbf79db),
            ("meta", 0x75715e),
            ("selector-tag", 0xf92672),
            ("selector-attr", 0xa6e22e),
            ("selector-pseudo", 0xa6e22e),
            ("name", 0xf92672),
            ("subst", 0xa6e22e),
            ("addition", 0xa6e22e),
            ("deletion", 0x75715e),
            ("tag", 0xf92672),
            ("regexp", 0xbf79db),
            ("section", 0xa6e22e),
            ("strong", 0xf92672),
            ("emphasis", 0xa6e22e),
            ("code", 0x66d9ef),
            ("symbol", 0xbf79db),
            ("bullet", 0xa6e22e),
        ],
        backgrounds: &[],
        modifiers: &[
            ("keyword", 1),
            ("title", 1),
            ("literal", 1),
            ("type", 1),
            ("selector-tag", 1),
            ("section", 1),
            ("doctag", 1),
            ("selector-id", 1),
        ],
    },
    EditorSyntaxPalette {
        name: "Tokyo Night Dark",
        background: 0x1a1b26,
        foreground: 0x9aa5ce,
        tokens: &[
            ("keyword", 0xbb9af7),
            ("template-tag", 0xf7768e),
            ("operator", 0xbb9af7),
            ("title", 0x7dcfff),
            ("title.function_", 0x7dcfff),
            ("string", 0x9ece6a),
            ("comment", 0x565f89),
            ("quote", 0x9ece6a),
            ("number", 0xff9e64),
            ("literal", 0xff9e64),
            ("built_in", 0xe0af68),
            ("variable", 0xff9e64),
            ("params", 0xff9e64),
            ("type", 0xff9e64),
            ("title.class_", 0x7dcfff),
            ("attr", 0xbb9af7),
            ("attribute", 0xe0af68),
            ("meta", 0x565f89),
            ("selector-tag", 0x73daca),
            ("selector-class", 0xf7768e),
            ("selector-attr", 0xf7768e),
            ("selector-pseudo", 0xf7768e),
            ("name", 0xbb9af7),
            ("subst", 0x7dcfff),
            ("addition", 0x9ece6a),
            ("deletion", 0xf7768e),
            ("tag", 0xf7768e),
            ("regexp", 0xf7768e),
            ("property", 0x7dcfff),
            ("punctuation", 0xc0caf5),
            ("section", 0x7aa2f7),
            ("doctag", 0xf7768e),
            ("code", 0x7aa2f7),
            ("symbol", 0x9ece6a),
            ("selector-id", 0xf7768e),
            ("bullet", 0x9ece6a),
        ],
        backgrounds: &[],
        modifiers: &[("strong", 1), ("emphasis", 2)],
    },
    EditorSyntaxPalette {
        name: "Dracula",
        background: 0x282936,
        foreground: 0xe9e9f4,
        tokens: &[
            ("keyword", 0xb45bcf),
            ("meta-keyword", 0x00f769),
            ("template-tag", 0xb45bcf),
            ("operator", 0xe9e9f4),
            ("title", 0x00f769),
            ("title.function_", 0x62d6e8),
            ("string", 0xebff87),
            ("meta-string", 0x00f769),
            ("comment", 0x626483),
            ("quote", 0xa1efe4),
            ("number", 0xb45bcf),
            ("literal", 0xb45bcf),
            ("built_in", 0xa1efe4),
            ("variable", 0xea51b2),
            ("type", 0xb45bcf),
            ("title.class_", 0x00f769),
            ("class-title", 0x00f769),
            ("attr", 0xb45bcf),
            ("attribute", 0x62d6e8),
            ("meta", 0x00f769),
            ("selector-tag", 0xea51b2),
            ("name", 0xea51b2),
            ("subst", 0xe9e9f4),
            ("addition", 0xebff87),
            ("deletion", 0xea51b2),
            ("tag", 0x62d6e8),
            ("regexp", 0xa1efe4),
            ("punctuation", 0xe9e9f4),
            ("section", 0x62d6e8),
            ("doctag", 0xa1efe4),
            ("strong", 0x00f769),
            ("emphasis", 0xb45bcf),
            ("code", 0xebff87),
            ("symbol", 0xb45bcf),
            ("bullet", 0xea51b2),
        ],
        backgrounds: &[],
        modifiers: &[("meta-keyword", 1), ("strong", 1), ("emphasis", 2)],
    },
];

/// Historical/unknown Host values use Classic's default Alera syntax theme.
pub fn palette_for_name(name: &str) -> &'static EditorSyntaxPalette {
    CLASSIC_EDITOR_SYNTAX_PALETTES
        .iter()
        .find(|theme| theme.name.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(&CLASSIC_EDITOR_SYNTAX_PALETTES[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_menu_catalog::EDITOR_SYNTAX_THEME_CHOICES;
    #[test]
    fn exact_classic_names_order_and_fallback() {
        assert_eq!(
            CLASSIC_EDITOR_SYNTAX_PALETTES.len(),
            EDITOR_SYNTAX_THEME_CHOICES.len()
        );
        for (theme, name) in CLASSIC_EDITOR_SYNTAX_PALETTES
            .iter()
            .zip(EDITOR_SYNTAX_THEME_CHOICES)
        {
            assert_eq!(theme.name, *name);
        }
        assert_eq!(palette_for_name(" unknown ").name, "Alera");
        assert_eq!(
            palette_for_name("  tOkYo NiGhT DaRk ").name,
            "Tokyo Night Dark"
        );
    }
    #[test]
    fn classic_keyword_and_string_colors_are_distinct_across_themes() {
        let alera = palette_for_name("Alera");
        let night = palette_for_name("Tokyo Night Dark");
        assert_eq!(alera.background, 0x101010);
        assert_eq!(alera.color("keyword"), Some(0xc792ea));
        assert_eq!(night.background, 0x1a1b26);
        assert_eq!(night.color("keyword"), Some(0xbb9af7));
        assert_eq!(night.color("string"), Some(0x9ece6a));
        assert_ne!(alera.color("string"), night.color("string"));
    }
    #[test]
    fn classic_font_weight_and_italic_modifiers_are_preserved() {
        let monokai = palette_for_name("Monokai");
        assert_eq!(monokai.modifier("keyword"), Some(1));
        assert_eq!(monokai.modifier("title"), Some(1));
        let night = palette_for_name("Night Owl");
        assert_eq!(night.modifier("keyword"), Some(2));
        assert_eq!(night.modifier("comment"), Some(2));
        let atom = palette_for_name("Atom One Dark");
        assert_eq!(atom.modifier("comment"), Some(2));
        assert_eq!(atom.modifier("strong"), Some(1));
        assert_eq!(palette_for_name("Alera").modifier("keyword"), None);
    }
    #[test]
    fn classic_diff_backgrounds_survive_generation() {
        let github = palette_for_name("GitHub Dark");
        assert_eq!(github.background("addition"), Some(0x033a16));
        assert_eq!(github.background("deletion"), Some(0x67060c));
        let dimmed = palette_for_name("GitHub Dark Dimmed");
        assert_eq!(dimmed.background("addition"), Some(0x1b4721));
        assert_eq!(dimmed.background("deletion"), Some(0x78191b));
        assert!(palette_for_name("Alera").background("addition").is_none());
    }
}
