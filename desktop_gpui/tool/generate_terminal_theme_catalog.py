#!/usr/bin/env python3
"""Generate GPUI terminal palettes directly from Classic's reviewed Dart catalog.

Keep the source-of-truth in lib/src/features/settings/domain/terminal_theme_*.dart
and alera_tokens.dart. Run with --check in CI to detect palette drift.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

PACKAGE = Path(__file__).resolve().parents[1]
ROOT = PACKAGE.parent
CLASSIC = ROOT / "lib" / "src" / "features" / "settings" / "domain"
TOKENS = ROOT / "lib" / "src" / "app" / "theme" / "alera_tokens.dart"
CATALOG = PACKAGE / "src" / "settings_menu_catalog.rs"
OUTPUT = PACKAGE / "src" / "terminal_theme.rs"
CHANNELS = ("black", "red", "green", "yellow", "blue", "magenta", "cyan", "white")


def call_body(text: str, left_paren: int) -> str:
    if text[left_paren] != "(":
        raise ValueError("Expected opening parenthesis")
    level = 0
    quote = ""
    for index in range(left_paren, len(text)):
        char = text[index]
        if quote:
            if char == quote and text[index - 1] != "\\":
                quote = ""
        elif char in ("'", '"'):
            quote = char
        elif char == "(":
            level += 1
        elif char == ")":
            level -= 1
            if level == 0:
                return text[left_paren + 1:index]
    raise ValueError("Unterminated Dart call")


def hex_arg(body: str, key: str, required: bool = True) -> int | None:
    match = re.search(r"\b" + re.escape(key) + r":\s*0x([0-9a-fA-F]+)", body)
    if not match:
        if required:
            raise ValueError("Missing hex parameter: " + key)
        return None
    return int(match.group(1), 16)


def ansi_colors(body: str) -> list[int]:
    normal = []
    bright = []
    for key in CHANNELS:
        match = re.search(
            r"\b" + key +
            r":\s*_AnsiColor\(\s*0x([a-fA-F0-9]+)(?:\s*,\s*0x([a-fA-F0-9]+))?\s*\)",
            body,
        )
        if not match:
            raise ValueError("Missing ANSI color: " + key)
        normal.append(int(match.group(1), 16))
        bright.append(int(match.group(2), 16) if match.group(2) else normal[-1])
    return normal + bright


def parse_palettes() -> list[dict]:
    catalog_src = (CLASSIC / "terminal_theme_catalog.dart").read_text(encoding="utf-8")
    all_src = "".join(
        (CLASSIC / filename).read_text(encoding="utf-8")
        for filename in ("terminal_theme_base_palette.dart", "terminal_theme_extended_palette.dart")
    )
    named = dict(re.findall(r"static const String (\w+) = '([^']+)';", catalog_src))
    aliases = {}
    for match in re.finditer(r"const _AnsiPalette (_\w+) = _AnsiPalette\(", catalog_src):
        body = call_body(catalog_src, match.end() - 1)
        aliases[match.group(1)] = ansi_colors(body)

    def parse_ansi(theme: str) -> list[int]:
        inline = re.search(r"\bansi:\s*(?:const\s+)?_AnsiPalette\(", theme)
        if inline:
            return ansi_colors(call_body(theme, inline.end() - 1))
        alias = re.search(r"\bansi:\s*(_\w+)", theme)
        if alias and alias.group(1) in aliases:
            return aliases[alias.group(1)]
        raise ValueError("Unknown ANSI palette in theme")

    result = []
    for match in re.finditer(r"_hexTheme\(", all_src):
        body = call_body(all_src, match.end() - 1)
        name_expr = body.split(",", 1)[0].strip()
        if name_expr.startswith("TerminalThemeNames."):
            name = named[name_expr.split(".", 1)[1]]
        elif name_expr.startswith("'") and name_expr.endswith("'"):
            name = name_expr[1:-1]
        else:
            raise ValueError("Unknown Classic theme name: " + name_expr)
        background = hex_arg(body, "background")
        foreground = hex_arg(body, "foreground")
        result.append(dict(
            name=name, background=background, foreground=foreground,
            cursor=hex_arg(body, "cursor", False) or foreground,
            selection=hex_arg(body, "selection"),
            ansi=parse_ansi(body),
        ))

    token_src = TOKENS.read_text(encoding="utf-8")
    tokens = dict(
        (key, int(value, 16))
        for key, value in re.findall(
            r"static const Color (\w+) = Color\(0x([a-fA-F0-9]+)\);", token_src
        )
    )
    def opaque(key: str) -> int:
        return tokens[key] & 0xFFFFFF
    def blend_on_background(key: str, background: int) -> int:
        raw = tokens[key]
        alpha = (raw >> 24) & 0xFF
        if alpha == 255:
            return raw & 0xFFFFFF
        front = raw & 0xFFFFFF
        channels = []
        for shift in (16, 8, 0):
            a = (front >> shift) & 0xFF
            b = (background >> shift) & 0xFF
            channels.append((a * alpha + b * (255 - alpha) + 127) // 255)
        return (channels[0] << 16) | (channels[1] << 8) | channels[2]
    bg = opaque("bg")
    alera = dict(
        name=named["aleraDark"],
        background=bg,
        foreground=opaque("foreground"),
        cursor=opaque("accent"),
        selection=blend_on_background("accentSubtle", bg),
        ansi=[opaque(x) for x in (
            "foregroundFaint", "error", "success", "warning",
            "info", "accent", "info", "foreground",
            "foregroundMuted", "error", "success", "warning",
            "info", "accent", "info", "foreground",
        )],
    )
    result.insert(0, alera)

    catalog = CATALOG.read_text(encoding="utf-8")
    choices_match = re.search(r"pub const TERMINAL_THEME_CHOICES:.*?=\s*&\[(.*?)\];", catalog, re.S)
    if not choices_match:
        raise ValueError("Missing GPUI terminal choices")
    choices = re.findall(r'"([^"]+)"', choices_match.group(1))
    if [theme["name"] for theme in result] != choices:
        raise ValueError("Classic theme order/names differ from GPUI picker")
    if len(result) != 31 or len(set(choices)) != 31:
        raise ValueError("Expected 31 distinct Classic themes")
    return result


def generate(themes: list[dict]) -> str:
    lines = [
        "//! GENERATED from Classic Flutter terminal_theme_*_palette.dart and alera_tokens.dart.",
        "//! Regenerate with python desktop_gpui/tool/generate_terminal_theme_catalog.py.",
        "//! Do not hand-edit; this is the shared visual projection, not settings storage.",
        "",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct TerminalTheme {",
        "    pub name: &'static str,",
        "    pub background: u32,",
        "    pub foreground: u32,",
        "    pub cursor: u32,",
        "    pub selection: u32,",
        "    pub ansi: [u32; 16],",
        "}",
        "",
        "pub const CLASSIC_TERMINAL_THEMES: &[TerminalTheme] = &[",
    ]
    for theme in themes:
        colors = ["0x%06x" % color for color in theme["ansi"]]
        lines += [
            "    TerminalTheme {",
            "        name: " + repr(theme["name"]).replace("'", '"') + ",",
            "        background: 0x%06x," % theme["background"],
            "        foreground: 0x%06x," % theme["foreground"],
            "        cursor: 0x%06x," % theme["cursor"],
            "        selection: 0x%06x," % theme["selection"],
            "        ansi: [",
            "            " + ", ".join(colors[:8]) + ",",
            "            " + ", ".join(colors[8:]) + ",",
            "        ],",
            "    },",
        ]
    lines += [
        "];",
        "",
        "/// Unknown or older Host theme names fall back to Classic Alera Dark.",
        "pub fn theme_for_name(name: &str) -> &'static TerminalTheme {",
        "    CLASSIC_TERMINAL_THEMES",
        "        .iter()",
        "        .find(|theme| theme.name.eq_ignore_ascii_case(name.trim()))",
        "        .unwrap_or(&CLASSIC_TERMINAL_THEMES[0])",
        "}",
        "",
        "#[cfg(test)]",
        "mod tests {",
        "    use super::*;",
        "    use crate::settings_menu_catalog::TERMINAL_THEME_CHOICES;",
        "",
        "    #[test]",
        "    fn catalog_matches_classic_options_in_order() {",
        "        assert_eq!(CLASSIC_TERMINAL_THEMES.len(), TERMINAL_THEME_CHOICES.len());",
        "        for (theme, expected) in CLASSIC_TERMINAL_THEMES.iter().zip(TERMINAL_THEME_CHOICES) {",
        "            assert_eq!(theme.name, *expected);",
        "        }",
        "    }",
        "",
        "    #[test]",
        "    fn known_dark_and_light_palettes_match_classic() {",
        "        let dracula = theme_for_name(\"Dracula\");",
        "        assert_eq!(dracula.background, 0x282a36);",
        "        assert_eq!(dracula.foreground, 0xf8f8f2);",
        "        assert_eq!(dracula.ansi[1], 0xff5555);",
        "        assert_eq!(dracula.ansi[9], 0xff6e6e);",
        "        let light = theme_for_name(\"GitHub Light\");",
        "        assert_eq!(light.background, 0xffffff);",
        "        assert_eq!(light.foreground, 0x24292e);",
        "        assert_eq!(light.ansi[1], 0xd73a49);",
        "        assert_eq!(light.ansi[9], 0xcb2431);",
        "        assert_eq!(theme_for_name(\"Alera Dark\").background, 0x101010);",
        "        assert_eq!(theme_for_name(\"unknown\").name, \"Alera Dark\");",
        "        assert_eq!(theme_for_name(\"  dRaCuLa \").name, \"Dracula\");",
        "    }",
        "}",
        "",
    ]
    return "\n".join(lines)


def main() -> None:
    generated = generate(parse_palettes())
    if "--check" in sys.argv:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != generated:
            raise SystemExit("Generated GPUI terminal theme catalog is stale")
        print("Checked Classic terminal theme palette parity: 31/31")
    else:
        OUTPUT.write_text(generated, encoding="utf-8")
        print("Generated Classic terminal theme palettes: 31/31")


if __name__ == "__main__":
    main()
