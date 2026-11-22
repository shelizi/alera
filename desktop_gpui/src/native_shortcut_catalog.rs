//! Searchable read-only inventory of common GPUI keyboard shortcuts.
//! Entries describe bindings registered in main.rs, editor_ui.rs, and
//! editor_find_ui.rs. This is NOT Classic's editable keyboard binding store.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeShortcut {
    pub scope: &'static str,
    pub action: &'static str,
    pub windows: &'static str,
    pub mac: &'static str,
}

// Keep key names identical to the actual GPUI KeyBinding::new registrations.
pub const COMMON_NATIVE_SHORTCUTS: &[NativeShortcut] = &[
    NativeShortcut {
        scope: "Global",
        action: "Quick Open files",
        windows: "Ctrl+P",
        mac: "Cmd+P",
    },
    NativeShortcut {
        scope: "Global",
        action: "Search contents",
        windows: "Ctrl+Shift+F",
        mac: "Cmd+Shift+F",
    },
    NativeShortcut {
        scope: "Global",
        action: "Search Agent history",
        windows: "Ctrl+Alt+G",
        mac: "Cmd+Alt+G",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "New file",
        windows: "Ctrl+N",
        mac: "Cmd+N",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "New folder",
        windows: "Ctrl+Shift+N",
        mac: "Cmd+Shift+N",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Rename",
        windows: "F2",
        mac: "F2",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Refresh files",
        windows: "F5",
        mac: "F5",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Delete",
        windows: "Delete",
        mac: "Delete",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Preview file",
        windows: "Space",
        mac: "Space",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Open selected",
        windows: "Enter",
        mac: "Enter",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Copy",
        windows: "Ctrl+C",
        mac: "Cmd+C",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Cut",
        windows: "Ctrl+X",
        mac: "Cmd+X",
    },
    NativeShortcut {
        scope: "Explorer",
        action: "Paste",
        windows: "Ctrl+V",
        mac: "Cmd+V",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Save",
        windows: "Ctrl+S",
        mac: "Cmd+S",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Save As",
        windows: "Ctrl+Shift+S",
        mac: "Cmd+Shift+S",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Undo",
        windows: "Ctrl+Z",
        mac: "Cmd+Z",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Redo",
        windows: "Ctrl+Y",
        mac: "Cmd+Shift+Z",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Find",
        windows: "Ctrl+F",
        mac: "Cmd+F",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Replace",
        windows: "Ctrl+H",
        mac: "Cmd+Alt+F",
    },
    NativeShortcut {
        scope: "Editor",
        action: "New untitled tab",
        windows: "Ctrl+N",
        mac: "Cmd+N",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Close tab",
        windows: "Ctrl+W",
        mac: "Cmd+W",
    },
    NativeShortcut {
        scope: "Editor",
        action: "Next tab",
        windows: "Ctrl+Tab",
        mac: "Cmd+Alt+Right",
    },
];

pub fn search_native_shortcuts(query: &str) -> Vec<&'static NativeShortcut> {
    let tokens = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    COMMON_NATIVE_SHORTCUTS
        .iter()
        .filter(|shortcut| {
            tokens.iter().all(|token| {
                [
                    &shortcut.action,
                    &shortcut.scope,
                    &shortcut.windows,
                    &shortcut.mac,
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(token))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_shortcut_search_matches_action_scope_platform_and_multiword() {
        assert_eq!(search_native_shortcuts("editor save").len(), 2);
        assert_eq!(search_native_shortcuts("ctrl+h")[0].action, "Replace");
        assert_eq!(
            search_native_shortcuts("cmd+shift+f")[0].action,
            "Search contents"
        );
        assert_eq!(search_native_shortcuts("explorer f2")[0].action, "Rename");
        assert_eq!(
            search_native_shortcuts("global agent history")[0].windows,
            "Ctrl+Alt+G"
        );
        assert_eq!(
            search_native_shortcuts("cmd+alt+g")[0].action,
            "Search Agent history"
        );
        assert!(search_native_shortcuts("never-mapped-shortcut").is_empty());
        assert_eq!(
            search_native_shortcuts(" ").len(),
            COMMON_NATIVE_SHORTCUTS.len()
        );
    }

    #[test]
    fn advertised_bindings_exist_in_real_registration_sources() {
        let sources = [
            include_str!("main.rs"),
            include_str!("editor_ui.rs"),
            include_str!("editor_find_ui.rs"),
        ];
        for row in COMMON_NATIVE_SHORTCUTS {
            for binding in [row.windows, row.mac] {
                let registration =
                    format!("\"{}\"", binding.to_ascii_lowercase().replace('+', "-"));
                assert!(
                    sources.iter().any(|source| source.contains(&registration)),
                    "Missing actual GPUI key registration for {}: {}",
                    row.action,
                    binding,
                );
            }
        }
    }
}
