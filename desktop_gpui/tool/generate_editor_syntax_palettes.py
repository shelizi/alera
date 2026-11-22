#!/usr/bin/env python3
"""Project Classic Editor syntax colors into Native GPUI without a new settings store.

The checked-in palette snapshot comes from the Classic Flutter re_highlight 0.0.3
style catalog. --import-styles refreshes that snapshot when the package is
available locally; normal generation/check never needs the package.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

PACKAGE = Path(__file__).resolve().parents[1]
ROOT = PACKAGE.parent
CLASSIC = ROOT / "lib/src/features/settings/domain/editor_syntax_theme_catalog.dart"
TOKENS = ROOT / "lib/src/app/theme/alera_tokens.dart"
MENU = PACKAGE / "src/settings_menu_catalog.rs"
SNAPSHOT = Path(__file__).with_name("editor_syntax_palette.json")
OUT = PACKAGE / "src/editor_syntax_theme.rs"
NAMES = [
    ("Alera", None),
    ("GitHub Dark", "github-dark"),
    ("GitHub Dark Dimmed", "github-dark-dimmed"),
    ("VS2015", "vs2015"),
    ("Atom One Dark", "atom-one-dark"),
    ("Night Owl", "night-owl"),
    ("Nord", "nord"),
    ("Monokai", "monokai"),
    ("Tokyo Night Dark", "tokyo-night-dark"),
    ("Dracula", "base16/dracula"),
]
CATEGORIES = (
    "root", "keyword", "meta-keyword", "template-tag", "operator",
    "function", "title", "title.function_", "string", "meta-string",
    "comment", "quote", "number", "literal", "built_in", "variable",
    "params", "type", "title.class_", "class-title", "attr", "attribute",
    "meta", "selector-tag", "selector-class", "selector-attr",
    "selector-pseudo", "name", "subst", "addition", "deletion",
    "tag", "regexp", "property", "punctuation", "section", "doctag",
    "strong", "emphasis", "code", "symbol", "selector-id", "bullet",
)
COLOR = re.compile(r"0x([a-fA-F0-9]{6,8})")
STYLE = re.compile(r"'([^']+)'\s*:\s*(?:const\s+)?TextStyle\(", re.S)


def call_body(src: str, begin: int) -> str:
    depth = 0
    for i in range(begin, len(src)):
        ch = src[i]
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return src[begin + 1:i]
    raise ValueError("Unclosed Dart TextStyle")


def properties(src: str, tokens: dict[str, int]) -> dict[str, dict[str, int]]:
    styles: dict[str, dict[str, int]] = {}
    for match in STYLE.finditer(src):
        key = match.group(1)
        if key not in CATEGORIES:
            continue
        body = call_body(src, match.end() - 1)
        values = {}
        for field, json_field in (("backgroundColor", "bg"), ("color", "fg")):
            found = re.search(r"\b" + field + r":\s*(?:Color\(0x([a-fA-F0-9]{6,8})\)|AleraTokens\.(\w+))", body)
            if found:
                if found.group(1):
                    values[json_field] = int(found.group(1), 16) & 0xFFFFFF
                elif found.group(2) in tokens:
                    values[json_field] = tokens[found.group(2)]
        # Style-only Classic entries (e.g. Markdown strong/emphasis) must
        # survive even without an explicit color.
        if re.search(r"\bfontWeight:\s*FontWeight\.(?:bold|w[7-9]00)\b", body):
            values["bold"] = True
        if re.search(r"\bfontStyle:\s*FontStyle\.italic\b", body):
            values["italic"] = True
        if values:
            styles[key] = values
    return styles


def load_tokens() -> dict[str, int]:
    src = TOKENS.read_text(encoding="utf-8")
    return {
        key: int(hex_value, 16) & 0xFFFFFF
        for key, hex_value in re.findall(
            r"static const Color (\w+) = Color\(0x([a-fA-F0-9]+)\);", src
        )
    }


def import_styles(style_dir: Path) -> dict:
    classic_src = CLASSIC.read_text(encoding="utf-8")
    alera = classic_src.split("const Map<String, TextStyle> _aleraEditorSyntaxTheme =", 1)[1]
    tokens = load_tokens()
    result = {"source": "Classic Flutter re_highlight 0.0.3; source filenames retained", "themes": []}
    for name, file_stem in NAMES:
        src = alera if file_stem is None else (style_dir / (file_stem + ".dart")).read_text(encoding="utf-8")
        values = properties(src, tokens)
        if "root" not in values or not values["root"].get("fg"):
            raise ValueError(f"Missing Classic root foreground for {name}")
        if len(values) < 7:
            raise ValueError(f"Too few parsed syntax colors for {name}: {len(values)}")
        result["themes"].append({"name": name, "source": file_stem or "_aleraEditorSyntaxTheme", "styles": values})
    return result


def check_names() -> None:
    source = MENU.read_text(encoding="utf-8")
    match = re.search(r"pub const EDITOR_SYNTAX_THEME_CHOICES:.*?=\s*&\[(.*?)\];", source, re.S)
    if match is None:
        raise ValueError("Missing GPUI Editor Syntax theme catalog")
    names = re.findall(r'"([^"]+)"', match.group(1))
    classic = CLASSIC.read_text(encoding="utf-8")
    listed = re.findall(
        r"static const String \w+ = '([^']+)';",
        classic.split("class const EditorSyntaxThemeEntry", 1)[0],
    )
    if names != listed or names != [name for name, _ in NAMES]:
        raise ValueError("Classic and GPUI Editor Syntax theme names/order disagree")


def render(snapshot: dict) -> str:
    themes = snapshot["themes"]
    if [obj["name"] for obj in themes] != [name for name, _ in NAMES]:
        raise ValueError("Snapshot no longer matches Classic Editor themes")
    a = [
        "//! Generated from Classic editor_syntax_theme_catalog.dart and re_highlight 0.0.3 colors.",
        "//! Regenerate with python desktop_gpui/tool/generate_editor_syntax_palettes.py.",
        "//! Colors describe Classic tokens; Syntect remains the GPUI syntax parser.",
        "",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct EditorSyntaxPalette {",
        "    pub name: &'static str,",
        "    pub background: u32,",
        "    pub foreground: u32,",
        "    pub tokens: &'static [(&'static str, u32)],",
        "    /// Optional per-token backgrounds (e.g. Classic diff addition/deletion).",
        "    pub backgrounds: &'static [(&'static str, u32)],",
        "    /// Classic TextStyle weights: bit 0 bold, bit 1 italic.",
        "    pub modifiers: &'static [(&'static str, u8)],",
        "}",
        "",
        "impl EditorSyntaxPalette {",
        "    pub fn color(&self, category: &str) -> Option<u32> {",
        "        if category == \"root\" { return Some(self.foreground); }",
        "        self.tokens.iter().find(|(name, _)| *name == category).map(|(_, color)| *color)",
        "    }",
        "    pub fn first_color(&self, categories: &[&str]) -> Option<u32> {",
        "        categories.iter().find_map(|name| self.color(name))",
        "    }",
        "    pub fn background(&self, category: &str) -> Option<u32> {",
        "        self.backgrounds.iter().find(|(name, _)| *name == category).map(|(_, color)| *color)",
        "    }",
        "    pub fn first_background(&self, categories: &[&str]) -> Option<u32> {",
        "        categories.iter().find_map(|name| self.background(name))",
        "    }",
        "    pub fn modifier(&self, category: &str) -> Option<u8> {",
        "        self.modifiers.iter().find(|(name, _)| *name == category).map(|(_, bits)| *bits)",
        "    }",
        "    pub fn first_modifier(&self, categories: &[&str]) -> Option<u8> {",
        "        categories.iter().find_map(|name| self.modifier(name))",
        "    }",
        "}",
        "",
        "pub const CLASSIC_EDITOR_SYNTAX_PALETTES: &[EditorSyntaxPalette] = &[",
    ]
    for obj in themes:
        styles = obj["styles"]
        root = styles["root"]
        fg = root["fg"]
        # For themes without their own root background, Flutter falls back to
        # Alera's editor canvas (not the terminal theme background).
        bg = root.get("bg", 0x101010)
        a += [
            "    EditorSyntaxPalette {",
            f"        name: {json.dumps(obj['name'])},",
            f"        background: 0x{bg:06x},",
            f"        foreground: 0x{fg:06x},",
            "        tokens: &[",
        ]
        for key in CATEGORIES:
            if key == "root":
                continue
            style = styles.get(key, {})
            if "fg" in style:
                a.append(f"            ({json.dumps(key)}, 0x{style['fg']:06x}),")
        a += ["        ],", "        backgrounds: &["]
        for key in CATEGORIES:
            if key == "root":
                continue
            style = styles.get(key, {})
            if "bg" in style:
                a.append(f"            ({json.dumps(key)}, 0x{style['bg']:06x}),")
        a += ["        ],", "        modifiers: &["]
        for key in CATEGORIES:
            style = styles.get(key, {})
            flags = (1 if style.get("bold") else 0) | (2 if style.get("italic") else 0)
            if flags:
                a.append(f"            ({json.dumps(key)}, {flags}),")
        a += ["        ],", "    },"]
    a += [
        "];",
        "",
        "/// Historical/unknown Host values use Classic's default Alera syntax theme.",
        "pub fn palette_for_name(name: &str) -> &'static EditorSyntaxPalette {",
        "    CLASSIC_EDITOR_SYNTAX_PALETTES",
        "        .iter()",
        "        .find(|theme| theme.name.eq_ignore_ascii_case(name.trim()))",
        "        .unwrap_or(&CLASSIC_EDITOR_SYNTAX_PALETTES[0])",
        "}",
        "",
        "#[cfg(test)]",
        "mod tests {",
        "    use super::*;",
        "    use crate::settings_menu_catalog::EDITOR_SYNTAX_THEME_CHOICES;",
        "    #[test]",
        "    fn exact_classic_names_order_and_fallback() {",
        "        assert_eq!(CLASSIC_EDITOR_SYNTAX_PALETTES.len(), EDITOR_SYNTAX_THEME_CHOICES.len());",
        "        for (theme, name) in CLASSIC_EDITOR_SYNTAX_PALETTES.iter().zip(EDITOR_SYNTAX_THEME_CHOICES) {",
        "            assert_eq!(theme.name, *name);",
        "        }",
        "        assert_eq!(palette_for_name(\" unknown \").name, \"Alera\");",
        "        assert_eq!(palette_for_name(\"  tOkYo NiGhT DaRk \").name, \"Tokyo Night Dark\");",
        "    }",
        "    #[test]",
        "    fn classic_keyword_and_string_colors_are_distinct_across_themes() {",
        "        let alera = palette_for_name(\"Alera\");",
        "        let night = palette_for_name(\"Tokyo Night Dark\");",
        "        assert_eq!(alera.background, 0x101010);",
        "        assert_eq!(alera.color(\"keyword\"), Some(0xc792ea));",
        "        assert_eq!(night.background, 0x1a1b26);",
        "        assert_eq!(night.color(\"keyword\"), Some(0xbb9af7));",
        "        assert_eq!(night.color(\"string\"), Some(0x9ece6a));",
        "        assert_ne!(alera.color(\"string\"), night.color(\"string\"));",
        "    }",
        "    #[test]",
        "    fn classic_font_weight_and_italic_modifiers_are_preserved() {",
        "        let monokai = palette_for_name(\"Monokai\");",
        "        assert_eq!(monokai.modifier(\"keyword\"), Some(1));",
        "        assert_eq!(monokai.modifier(\"title\"), Some(1));",
        "        let night = palette_for_name(\"Night Owl\");",
        "        assert_eq!(night.modifier(\"keyword\"), Some(2));",
        "        assert_eq!(night.modifier(\"comment\"), Some(2));",
        "        let atom = palette_for_name(\"Atom One Dark\");",
        "        assert_eq!(atom.modifier(\"comment\"), Some(2));",
        "        assert_eq!(atom.modifier(\"strong\"), Some(1));",
        "        assert_eq!(palette_for_name(\"Alera\").modifier(\"keyword\"), None);",
        "    }",
        "    #[test]",
        "    fn classic_diff_backgrounds_survive_generation() {",
        "        let github = palette_for_name(\"GitHub Dark\");",
        "        assert_eq!(github.background(\"addition\"), Some(0x033a16));",
        "        assert_eq!(github.background(\"deletion\"), Some(0x67060c));",
        "        let dimmed = palette_for_name(\"GitHub Dark Dimmed\");",
        "        assert_eq!(dimmed.background(\"addition\"), Some(0x1b4721));",
        "        assert_eq!(dimmed.background(\"deletion\"), Some(0x78191b));",
        "        assert!(palette_for_name(\"Alera\").background(\"addition\").is_none());",
        "    }",
        "}",
        "",
    ]
    rust = "\n".join(a)
    with tempfile.TemporaryDirectory() as temp:
        f = Path(temp) / "editor_syntax_theme.rs"
        f.write_text(rust, encoding="utf-8")
        proc = subprocess.run(["rustfmt", "--edition", "2021", str(f)], capture_output=True, text=True)
        if proc.returncode:
            raise ValueError(f"rustfmt rejected editor palette: {proc.stderr[:2400]}")
        return f.read_text(encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path, help="Optional re_highlight 0.0.3 lib/styles root")
    parser.add_argument("--import-styles", action="store_true", help="Import vendor palette snapshot")
    parser.add_argument("--check", action="store_true")
    options = parser.parse_args()
    check_names()
    if options.import_styles:
        if options.check:
            parser.error("--import-styles and --check cannot be combined")
        source_dir = options.source_dir
        if source_dir is None:
            source_dir = Path(os.environ["LOCALAPPDATA"]) / "Pub/Cache/hosted/pub.dev/re_highlight-0.0.3/lib/styles"
        SNAPSHOT.write_text(
            json.dumps(import_styles(source_dir), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    snapshot = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    generated = render(snapshot)
    if options.check:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != generated:
            raise SystemExit("Native Editor palettes differ from Classic snapshot")
        print("Checked Classic Editor syntax palettes: 10/10")
    else:
        OUT.write_text(generated, encoding="utf-8")
        print("Generated Classic Editor syntax palettes: 10/10")


if __name__ == "__main__":
    main()
