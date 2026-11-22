//! GENERATED from Classic Flutter terminal_theme_*_palette.dart and alera_tokens.dart.
//! Regenerate with python desktop_gpui/tool/generate_terminal_theme_catalog.py.
//! Do not hand-edit; this is the shared visual projection, not settings storage.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalTheme {
    pub name: &'static str,
    pub background: u32,
    pub foreground: u32,
    pub cursor: u32,
    pub selection: u32,
    pub ansi: [u32; 16],
}

pub const CLASSIC_TERMINAL_THEMES: &[TerminalTheme] = &[
    TerminalTheme {
        name: "Alera Dark",
        background: 0x101010,
        foreground: 0xf5f5f5,
        cursor: 0xe0e0e0,
        selection: 0x252525,
        ansi: [
            0x606060, 0xf87171, 0x22c55e, 0xf59e0b, 0x60a5fa, 0xe0e0e0, 0x60a5fa, 0xf5f5f5,
            0xa1a1a1, 0xf87171, 0x22c55e, 0xf59e0b, 0x60a5fa, 0xe0e0e0, 0x60a5fa, 0xf5f5f5,
        ],
    },
    TerminalTheme {
        name: "Ghostty Default Style Dark",
        background: 0x282c34,
        foreground: 0xffffff,
        cursor: 0xffffff,
        selection: 0x3e4451,
        ansi: [
            0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6,
            0x666666, 0xd54e53, 0xb9ca4a, 0xe7c547, 0x7aa6da, 0xc397d8, 0x70c0b1, 0xeaeaea,
        ],
    },
    TerminalTheme {
        name: "Builtin Tango Light",
        background: 0xffffff,
        foreground: 0x2e3434,
        cursor: 0x2e3434,
        selection: 0xaccef7,
        ansi: [
            0x2e3436, 0xcc0000, 0x4e9a06, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf,
            0x555753, 0xef2929, 0x8ae234, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
        ],
    },
    TerminalTheme {
        name: "Dracula",
        background: 0x282a36,
        foreground: 0xf8f8f2,
        cursor: 0xf8f8f2,
        selection: 0x44475a,
        ansi: [
            0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
            0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
        ],
    },
    TerminalTheme {
        name: "One Dark",
        background: 0x282c34,
        foreground: 0xabb2bf,
        cursor: 0x528bff,
        selection: 0x3e4451,
        ansi: [
            0x282c34, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
            0x5c6370, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xffffff,
        ],
    },
    TerminalTheme {
        name: "Nord",
        background: 0x2e3440,
        foreground: 0xd8dee9,
        cursor: 0xd8dee9,
        selection: 0x434c5e,
        ansi: [
            0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
            0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
        ],
    },
    TerminalTheme {
        name: "Monokai",
        background: 0x272822,
        foreground: 0xf8f8f2,
        cursor: 0xf8f8f0,
        selection: 0x49483e,
        ansi: [
            0x272822, 0xf92672, 0xa6e22e, 0xf4bf75, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xf8f8f2,
            0x75715e, 0xf92672, 0xa6e22e, 0xf4bf75, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xf9f8f5,
        ],
    },
    TerminalTheme {
        name: "Tokyo Night",
        background: 0x1a1b26,
        foreground: 0xc0caf5,
        cursor: 0xc0caf5,
        selection: 0x33467c,
        ansi: [
            0x15161e, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xa9b1d6,
            0x414868, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5,
        ],
    },
    TerminalTheme {
        name: "Gruvbox Dark",
        background: 0x282828,
        foreground: 0xebdbb2,
        cursor: 0xebdbb2,
        selection: 0x504945,
        ansi: [
            0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984,
            0x928374, 0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xebdbb2,
        ],
    },
    TerminalTheme {
        name: "Catppuccin Mocha",
        background: 0x1e1e2e,
        foreground: 0xcdd6f4,
        cursor: 0xf5e0dc,
        selection: 0x585b70,
        ansi: [
            0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de,
            0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xa6adc8,
        ],
    },
    TerminalTheme {
        name: "Solarized Dark",
        background: 0x002b36,
        foreground: 0x839496,
        cursor: 0x839496,
        selection: 0x073642,
        ansi: [
            0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ],
    },
    TerminalTheme {
        name: "Material Dark",
        background: 0x263238,
        foreground: 0xeeffff,
        cursor: 0xffcc00,
        selection: 0x546e7a,
        ansi: [
            0x000000, 0xf07178, 0xc3e88d, 0xffcb6b, 0x82aaff, 0xc792ea, 0x89ddff, 0xeeffff,
            0x546e7a, 0xf07178, 0xc3e88d, 0xffcb6b, 0x82aaff, 0xc792ea, 0x89ddff, 0xffffff,
        ],
    },
    TerminalTheme {
        name: "Ayu Dark",
        background: 0x0a0e14,
        foreground: 0xb3b1ad,
        cursor: 0xe6b450,
        selection: 0x273747,
        ansi: [
            0x01060e, 0xea6c73, 0x91b362, 0xf9af4f, 0x53bdfa, 0xfae994, 0x90e1c6, 0xc7c7c7,
            0x686868, 0xf07178, 0xc2d94c, 0xffb454, 0x59c2ff, 0xffee99, 0x95e6cb, 0xffffff,
        ],
    },
    TerminalTheme {
        name: "Nightfox",
        background: 0x192330,
        foreground: 0xcdcecf,
        cursor: 0xcdcecf,
        selection: 0x2b3b51,
        ansi: [
            0x393b44, 0xc94f6d, 0x81b29a, 0xdbc074, 0x719cd6, 0x9d79d6, 0x63cdcf, 0xdfdfe0,
            0x575860, 0xd16983, 0x8ebaa4, 0xe0c989, 0x86abdc, 0xbaa1e2, 0x7ad5d6, 0xe4e4e5,
        ],
    },
    TerminalTheme {
        name: "Kanagawa",
        background: 0x1f1f28,
        foreground: 0xdcd7ba,
        cursor: 0xc8c093,
        selection: 0x2d4f67,
        ansi: [
            0x090618, 0xc34043, 0x76946a, 0xc0a36e, 0x7e9cd8, 0x957fb8, 0x6a9589, 0xc8c093,
            0x727169, 0xe82424, 0x98bb6c, 0xe6c384, 0x7fb4ca, 0x938aa9, 0x7aa89f, 0xdcd7ba,
        ],
    },
    TerminalTheme {
        name: "Rose Pine",
        background: 0x191724,
        foreground: 0xe0def4,
        cursor: 0x524f67,
        selection: 0x403d52,
        ansi: [
            0x26233a, 0xeb6f92, 0x31748f, 0xf6c177, 0x9ccfd8, 0xc4a7e7, 0xebbcba, 0xe0def4,
            0x6e6a86, 0xeb6f92, 0x31748f, 0xf6c177, 0x9ccfd8, 0xc4a7e7, 0xebbcba, 0xe0def4,
        ],
    },
    TerminalTheme {
        name: "Everforest Dark",
        background: 0x2d353b,
        foreground: 0xd3c6aa,
        cursor: 0xd3c6aa,
        selection: 0x543a48,
        ansi: [
            0x475258, 0xe67e80, 0xa7c080, 0xdbbc7f, 0x7fbbb3, 0xd699b6, 0x83c092, 0xd3c6aa,
            0x697379, 0xe67e80, 0xa7c080, 0xdbbc7f, 0x7fbbb3, 0xd699b6, 0x83c092, 0xd3c6aa,
        ],
    },
    TerminalTheme {
        name: "Palenight",
        background: 0x292d3e,
        foreground: 0xa6accd,
        cursor: 0xffcc00,
        selection: 0x434758,
        ansi: [
            0x292d3e, 0xf07178, 0xc3e88d, 0xffcb6b, 0x82aaff, 0xc792ea, 0x89ddff, 0xa6accd,
            0x676e95, 0xf07178, 0xc3e88d, 0xffcb6b, 0x82aaff, 0xc792ea, 0x89ddff, 0xffffff,
        ],
    },
    TerminalTheme {
        name: "Horizon Dark",
        background: 0x1c1e26,
        foreground: 0xe0e0e0,
        cursor: 0xe95678,
        selection: 0x2e303e,
        ansi: [
            0x16161c, 0xe95678, 0x29d398, 0xfab795, 0x26bbd9, 0xee64ac, 0x59e1e3, 0xd5d8da,
            0x6c6f93, 0xec6a88, 0x3fdaa4, 0xfbc3a7, 0x3fc4de, 0xf075b5, 0x6be4e6, 0xf0f0f0,
        ],
    },
    TerminalTheme {
        name: "Night Owl",
        background: 0x011627,
        foreground: 0xd6deeb,
        cursor: 0x80a4c2,
        selection: 0x1d3b53,
        ansi: [
            0x011627, 0xef5350, 0x22da6e, 0xaddb67, 0x82aaff, 0xc792ea, 0x21c7a8, 0xffffff,
            0x575656, 0xef5350, 0x22da6e, 0xffeb95, 0x82aaff, 0xc792ea, 0x7fdbca, 0xffffff,
        ],
    },
    TerminalTheme {
        name: "Solarized Light",
        background: 0xfdf6e3,
        foreground: 0x657b83,
        cursor: 0x657b83,
        selection: 0xeee8d5,
        ansi: [
            0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ],
    },
    TerminalTheme {
        name: "One Light",
        background: 0xfafafa,
        foreground: 0x383a42,
        cursor: 0x526fff,
        selection: 0xe5e5e6,
        ansi: [
            0x383a42, 0xe45649, 0x50a14f, 0xc18401, 0x4078f2, 0xa626a4, 0x0184bc, 0xa0a1a7,
            0x696c77, 0xe45649, 0x50a14f, 0xc18401, 0x4078f2, 0xa626a4, 0x0184bc, 0xfafafa,
        ],
    },
    TerminalTheme {
        name: "Catppuccin Latte",
        background: 0xeff1f5,
        foreground: 0x4c4f69,
        cursor: 0xdc8a78,
        selection: 0xacb0be,
        ansi: [
            0x5c5f77, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xacb0be,
            0x6c6f85, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xbcc0cc,
        ],
    },
    TerminalTheme {
        name: "GitHub Light",
        background: 0xffffff,
        foreground: 0x24292e,
        cursor: 0x044289,
        selection: 0xc8c8fa,
        ansi: [
            0x24292e, 0xd73a49, 0x28a745, 0xdbab09, 0x0366d6, 0x5a32a3, 0x0598bc, 0x6a737d,
            0x959da5, 0xcb2431, 0x22863a, 0xb08800, 0x005cc5, 0x5a32a3, 0x3192aa, 0xd1d5da,
        ],
    },
    TerminalTheme {
        name: "Rose Pine Dawn",
        background: 0xfaf4ed,
        foreground: 0x575279,
        cursor: 0x9893a5,
        selection: 0xdfdad9,
        ansi: [
            0xf2e9e1, 0xb4637a, 0x286983, 0xea9d34, 0x56949f, 0x907aa9, 0xd7827e, 0x575279,
            0x9893a5, 0xb4637a, 0x286983, 0xea9d34, 0x56949f, 0x907aa9, 0xd7827e, 0x575279,
        ],
    },
    TerminalTheme {
        name: "Gruvbox Light",
        background: 0xfbf1c7,
        foreground: 0x3c3836,
        cursor: 0x3c3836,
        selection: 0xebdbb2,
        ansi: [
            0xfbf1c7, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0x7c6f64,
            0x928374, 0x9d0006, 0x79740e, 0xb57614, 0x076678, 0x8f3f71, 0x427b58, 0x3c3836,
        ],
    },
    TerminalTheme {
        name: "Tokyo Night Light",
        background: 0xd5d6db,
        foreground: 0x343b58,
        cursor: 0x343b58,
        selection: 0x9699a3,
        ansi: [
            0x0f0f14, 0x8c4351, 0x485e30, 0x8f5e15, 0x34548a, 0x5a4a78, 0x0f4b6e, 0x343b58,
            0x9699a3, 0x8c4351, 0x485e30, 0x8f5e15, 0x34548a, 0x5a4a78, 0x0f4b6e, 0x343b58,
        ],
    },
    TerminalTheme {
        name: "Everforest Light",
        background: 0xfdf6e3,
        foreground: 0x5c6a72,
        cursor: 0x5c6a72,
        selection: 0xe6e2cc,
        ansi: [
            0x5c6a72, 0xf85552, 0x8da101, 0xdfa000, 0x3a94c5, 0xdf69ba, 0x35a77c, 0xdfd9a8,
            0x939f91, 0xf85552, 0x8da101, 0xdfa000, 0x3a94c5, 0xdf69ba, 0x35a77c, 0xfdf6e3,
        ],
    },
    TerminalTheme {
        name: "Tango Dark",
        background: 0x000000,
        foreground: 0xd3d7cf,
        cursor: 0xd3d7cf,
        selection: 0x555753,
        ansi: [
            0x2e3436, 0xcc0000, 0x4e9a06, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf,
            0x555753, 0xef2929, 0x8ae234, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
        ],
    },
    TerminalTheme {
        name: "Homebrew",
        background: 0x000000,
        foreground: 0x00ff00,
        cursor: 0x00ff00,
        selection: 0x005500,
        ansi: [
            0x000000, 0x990000, 0x00a600, 0x999900, 0x0000b2, 0xb200b2, 0x00a6b2, 0xbfbfbf,
            0x666666, 0xe50000, 0x00d900, 0xe5e500, 0x0000ff, 0xe500e5, 0x00e5e5, 0xe5e5e5,
        ],
    },
    TerminalTheme {
        name: "Snazzy",
        background: 0x282a36,
        foreground: 0xeff0eb,
        cursor: 0x97979b,
        selection: 0x3e404a,
        ansi: [
            0x282a36, 0xff5c57, 0x5af78e, 0xf3f99d, 0x57c7ff, 0xff6ac1, 0x9aedfe, 0xf1f1f0,
            0x686868, 0xff5c57, 0x5af78e, 0xf3f99d, 0x57c7ff, 0xff6ac1, 0x9aedfe, 0xf1f1f0,
        ],
    },
];

/// Unknown or older Host theme names fall back to Classic Alera Dark.
pub fn theme_for_name(name: &str) -> &'static TerminalTheme {
    CLASSIC_TERMINAL_THEMES
        .iter()
        .find(|theme| theme.name.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(&CLASSIC_TERMINAL_THEMES[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_menu_catalog::TERMINAL_THEME_CHOICES;

    #[test]
    fn catalog_matches_classic_options_in_order() {
        assert_eq!(CLASSIC_TERMINAL_THEMES.len(), TERMINAL_THEME_CHOICES.len());
        for (theme, expected) in CLASSIC_TERMINAL_THEMES.iter().zip(TERMINAL_THEME_CHOICES) {
            assert_eq!(theme.name, *expected);
        }
    }

    #[test]
    fn known_dark_and_light_palettes_match_classic() {
        let dracula = theme_for_name("Dracula");
        assert_eq!(dracula.background, 0x282a36);
        assert_eq!(dracula.foreground, 0xf8f8f2);
        assert_eq!(dracula.ansi[1], 0xff5555);
        assert_eq!(dracula.ansi[9], 0xff6e6e);
        let light = theme_for_name("GitHub Light");
        assert_eq!(light.background, 0xffffff);
        assert_eq!(light.foreground, 0x24292e);
        assert_eq!(light.ansi[1], 0xd73a49);
        assert_eq!(light.ansi[9], 0xcb2431);
        assert_eq!(theme_for_name("Alera Dark").background, 0x101010);
        assert_eq!(theme_for_name("unknown").name, "Alera Dark");
        assert_eq!(theme_for_name("  dRaCuLa ").name, "Dracula");
    }
}
