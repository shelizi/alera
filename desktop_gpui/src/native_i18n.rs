//! Shared Native translation boundary.
//! Texts are taken from the Classic Alera localization vocabulary by
//! tool/generate_settings_locales.py. New pages can reuse tr() and UiLocale.
//!
//! Locale is passed explicitly to avoid global-state cross-window leaks;
//! when GPUI Kit owns more of the surface, synchronize component-level locale
//! separately and notify/repaint the owning view.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiLocale {
    #[default]
    English,
    TraditionalChinese,
    SimplifiedChinese,
    Japanese,
}

impl UiLocale {
    pub fn from_preference(raw: &str) -> Self {
        match raw {
            "english" => Self::English,
            "traditionalChinese" => Self::TraditionalChinese,
            "simplifiedChinese" => Self::SimplifiedChinese,
            "japanese" => Self::Japanese,
            "system" => Self::from_tag(&system_locale_name()),
            _ => Self::English,
        }
    }

    pub const ALL: [Self; 4] = [
        Self::English,
        Self::TraditionalChinese,
        Self::SimplifiedChinese,
        Self::Japanese,
    ];

    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::TraditionalChinese => "zh-TW",
            Self::SimplifiedChinese => "zh-CN",
            Self::Japanese => "ja",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::TraditionalChinese => "繁體中文",
            Self::SimplifiedChinese => "简体中文",
            Self::Japanese => "日本語",
        }
    }

    pub fn from_tag(raw: &str) -> Self {
        let tag = raw
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .replace('_', "-");
        let tag = tag.to_ascii_lowercase();
        match tag.as_str() {
            "zh-tw" | "zh-hant" | "zh-hant-tw" | "zh-hk" | "zh-mo" => Self::TraditionalChinese,
            "zh-cn" | "zh-hans" | "zh-hans-cn" | "zh-sg" | "zh" => Self::SimplifiedChinese,
            "ja" | "ja-jp" => Self::Japanese,
            _ => Self::English,
        }
    }
}

/// Resolve the same OS language as Classic for the persisted 'system' mode.
fn system_locale_name() -> String {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultLocaleName(name: *mut u16, count: i32) -> i32;
        }
        let mut buffer = [0u16; 85];
        let count = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
        if count > 1 {
            return String::from_utf16_lossy(&buffer[..count as usize - 1]);
        }
    }
    std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "en".into())
}

/// Translation of a Classic source string, with English as final fallback.
/// Callers should pass locale from the view state, not mutate global locale.
pub fn tr(locale: UiLocale, source: &str) -> String {
    rust_i18n::t!(source, locale = locale.tag()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_settings_terms_translate_across_all_supported_locales() {
        assert_eq!(tr(UiLocale::English, "Settings"), "Settings");
        assert_eq!(tr(UiLocale::TraditionalChinese, "Settings"), "設定");
        assert_eq!(tr(UiLocale::SimplifiedChinese, "Settings"), "设置");
        assert_eq!(tr(UiLocale::Japanese, "Settings"), "設定");
        assert_eq!(tr(UiLocale::TraditionalChinese, "Application"), "應用程式");
        assert_eq!(tr(UiLocale::Japanese, "Terminal"), "ターミナル");
        assert_eq!(
            tr(UiLocale::TraditionalChinese, "No matching settings."),
            "沒有符合的設定。"
        );
        assert_eq!(
            tr(
                UiLocale::Japanese,
                "UI preview only · controls are not connected to settings"
            ),
            "UI プレビューのみ · 設定への連携は未実装です"
        );
        assert_eq!(
            tr(UiLocale::TraditionalChinese, "Connected Native Controls"),
            "已連接的 Native 設定"
        );
    }

    #[test]
    fn sidebar_long_press_reorder_hint_is_localized_in_all_four_languages() {
        let source = "Hold the handle for 0.5s to reorder";
        assert_eq!(tr(UiLocale::English, source), source);
        assert_eq!(
            tr(UiLocale::TraditionalChinese, source),
            "長按把手 0.5 秒，再拖曳排序"
        );
        assert_eq!(
            tr(UiLocale::SimplifiedChinese, source),
            "长按把手 0.5 秒，再拖动排序"
        );
        assert_eq!(
            tr(UiLocale::Japanese, source),
            "ハンドルを0.5秒長押しして並べ替え"
        );
    }

    #[test]
    fn s04_shared_settings_operational_labels_have_real_translations() {
        let examples = [
            ("Not available in GPUI yet", "GPUI 尚未提供"),
            ("Loading Rust Host settings", "正在讀取 Rust Host 設定"),
            ("Rust Host settings unavailable", "無法取得 Rust Host 設定"),
            ("Open Quota Overview", "開啟額度總覽"),
            ("Manage Projects", "管理專案"),
            ("Editable", "可編輯設定"),
            ("Read-only", "唯讀"),
            ("Not available", "尚未提供"),
            ("Enabled", "已啟用"),
            ("Disabled", "已停用"),
        ];
        for (source, zh) in examples {
            assert_eq!(tr(UiLocale::TraditionalChinese, source), zh, "{source}");
            assert_eq!(tr(UiLocale::English, source), source, "{source}");
        }
    }
    #[test]
    fn three_way_git_conflict_labels_are_localized_in_every_ui_language() {
        for locale in UiLocale::ALL {
            for source in [
                "Merge conflict",
                "Close Preview",
                "Base (stage 1)",
                "Current · Ours (stage 2)",
                "Incoming · Theirs (stage 3)",
                "changed line(s)",
                "Git index stages 1/2/3 · read-only snapshot · no resolving or staging",
                "Simulated merge index · read-only snapshot · no file or index changes",
                "Click a conflict file to open full three-way Base / Ours / Theirs read-only preview.",
                "Conflict differences",
                "Previous difference",
                "Next difference",
                "Base version",
                "Ours version",
                "Theirs version",
                "Pin conflict comparison to workspace tab",
                "Show all lines",
                "Collapse unchanged lines",
                "unchanged lines hidden",
                "unchanged lines shown",
                "Expand this section",
                "Collapse this section",
                "Long unchanged sections are folded",
                "F7 next · Shift+F7 previous · Alt+U expand/collapse",
            ] {
                let value = tr(locale, source);
                assert!(!value.is_empty(), "Missing {}: {}", locale.tag(), source);
                if locale != UiLocale::English {
                    assert_ne!(value, source, "Missing {}: {}", locale.tag(), source);
                }
            }
        }
    }

    #[test]
    fn inner_split_terminal_header_is_localized_in_every_ui_language() {
        let source = include_str!("main.rs");
        let focus = source.split_once(r#".id("terminal-secondary-focus")"#).unwrap().1
            .split_once(".on_click(").unwrap().0;
        for phrase in ["Second pane", "Click to focus"] {
            assert!(focus.contains(&format!(r#"tr(self.settings_menu_locale, "{phrase}")"#)),
                "{phrase} must use the active workbench language");
            assert_eq!(tr(UiLocale::English, phrase), phrase);
            for locale in [UiLocale::TraditionalChinese, UiLocale::SimplifiedChinese, UiLocale::Japanese] {
                let translated = tr(locale, phrase);
                assert_ne!(translated, phrase, "{phrase} missing for {}", locale.tag());
                assert!(!translated.is_empty());
            }
        }
    }

    #[test]
    fn native_editor_toolbar_has_real_localized_labels_and_tooltips() {
        // Classic localizes the Editor shell. GPUI's toolbar must use the
        // same active language for every button and its status caption.
        let source = include_str!("main.rs");
        let editor_toolbar = source
            .split_once(".when(!show_editor_toolbar || show_center_git_diff").unwrap().1
            .split_once(r#".id("mixed-pane-layout")"#).unwrap().0;
        assert!(editor_toolbar.contains(r#"tr(self.settings_menu_locale, "Editing")"#));
        assert!(editor_toolbar.contains(r#"tr(self.settings_menu_locale, "read-only preview")"#));
        for label in [
            "Create new untitled editor",
            "Edit currently selected file",
            "Save current editor file (Ctrl+S)",
            "Save current editor as a different file",
            "Close current editor tab",
            "Discard unsaved changes (confirmation required)",
        ] {
            assert!(editor_toolbar.contains(&format!(r#"tr(self.settings_menu_locale, "{label}")"#)),
                "Native Editor tooltip must use current UI language: {label}");
        }
        for label in [
            "Editing",
            "read-only preview",
            "Create new untitled editor",
            "Edit currently selected file",
            "Save current editor file (Ctrl+S)",
            "Save current editor as a different file",
            "Close current editor tab",
            "Discard unsaved changes (confirmation required)",
        ] {
            assert_eq!(tr(UiLocale::English, label), label);
            for locale in [UiLocale::TraditionalChinese, UiLocale::SimplifiedChinese, UiLocale::Japanese] {
                let translated = tr(locale, label);
                assert_ne!(translated, label, "{label}: missing {}", locale.tag());
                assert!(!translated.trim().is_empty());
            }
        }
    }

    #[test]
    fn tab_context_and_pane_actions_have_real_translations_in_all_locales() {
        // The mixed Editor/Terminal/Agent Tab context menu and the adjacent
        // pane actions must never silently revert to English in zh-TW,
        // zh-CN or Japanese. Keep these labels in the Native locale catalog.
        let labels = [
            "Focus",
            "Pin Tab",
            "Unpin Tab",
            "Close",
            "Close others",
            "Close to right",
            "Move left",
            "Move right",
            "Move to first pane",
            "Move to second pane",
            "Tab changed; action cancelled",
            "New File in this pane",
            "Refresh output",
            "Copy visible terminal",
            "Copy selected rows",
            "Toggle VT screen / text log",
            "Resize: 80 × 24",
            "Resize: 120 × 36",
            "Toggle Terminal split",
            "Switch Terminal split direction",
            "Move tab to first pane",
            "Move tab to second pane",
            "Switch split direction",
            "Close split pane",
            "Open a tab to split",
        ];
        for source in labels {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                let translated = tr(locale, source);
                assert_ne!(translated, source, "{source} missing in {}", locale.tag());
                assert!(
                    !translated.trim().is_empty(),
                    "{source}: empty {}",
                    locale.tag()
                );
            }
        }
    }

    #[test]
    fn explorer_context_actions_have_real_translations_in_all_locales() {
        // ExplorerMenuAction::label() is the source of context menu labels.
        let labels = [
            "New file…",
            "New folder…",
            "Rename…",
            "Move to folder…",
            "Copy into folder…",
            "Duplicate",
            "Delete permanently…",
            "Copy selection",
            "Cut selection",
            "Paste",
            "Open in Alera",
            "Show in file manager",
            "View current Git changes",
            "Copy path",
            "Copy relative path",
            "Collapse folder",
            "Refresh",
            "File / folder Git history",
            "Explorer item changed; refresh the folder",
        ];
        for source in labels {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                assert_ne!(tr(locale, source), source, "{source} / {}", locale.tag());
            }
        }
        let popup = include_str!("native_popup_ui.rs");
        let explorer_menu = popup
            .split_once("pub(super) fn kit_explorer_context_menu(")
            .unwrap()
            .1
            .split_once("fn run_kit_explorer_menu_action(")
            .unwrap()
            .0;
        assert!(explorer_menu.contains("PopupMenuItem::new(tr(locale, action.label()))"));
    }

    #[test]
    fn explorer_action_dialog_and_feedback_translate_in_all_locales() {
        // Context menu localization includes dialogs, destructive confirmation,
        // completion/errors and the Explorer overflow menu, not menu items alone.
        let labels = [
            "Delete permanently",
            "New file",
            "New folder",
            "Rename",
            "Move into folder",
            "Copy into folder",
            "New file name",
            "New folder name",
            "New name",
            "Destination folder inside workspace",
            "Copy into destination folder inside workspace",
            "The selected files and folders (including contents) will be permanently removed, not moved to Recycle Bin. Review the selection before confirming.",
            "Create a copy of the selected item in its current folder.",
            "Enter the name or destination within the current workspace.",
            "Confirm permanent deletion ({count})",
            "Cancel",
            "Select a file or folder in Explorer first",
            "Close Editor tabs for the selected file or folder before Rename/Move",
            "Explorer operation succeeded: {action}{target}",
            "Explorer operation refused: {action}: {error}",
            "Permanently delete {count} selected item(s), including folder contents? This cannot be undone. Press CONFIRM PERMANENT DELETE to proceed.",
            "Close any Native Editor files inside the selection before permanent deletion",
            "Delete confirmation expired; choose the items again",
            "Delete selection or workspace changed; no files were deleted",
            "Permanently deleted {count} Explorer item(s)",
            "Explorer delete refused or partially completed: {error}. Check refreshed files.",
            "Select a file or folder to Copy or Cut",
            "Copied {count} Explorer item(s); choose destination and Paste",
            "Cut {count} Explorer item(s); choose destination and Paste",
            "Explorer Copy/Cut refused: {error}",
            "Explorer clipboard is empty; Copy or Cut a file first",
            "Explorer Paste succeeded: {count} item(s)",
            "Explorer Paste refused or stopped: {error}",
            "Opened system file manager",
            "Cannot reveal selected item: {error}",
            "Choose an existing Explorer item first",
            "Copied path relative to the project root",
            "Copied full file/folder path",
            "Collapse folder refused: {error}",
            "Explorer refresh failed: {error}",
            "Tree refresh failed: {error}",
            "Show ignored files",
            "Hide ignored files",
            "Show hidden items",
            "Hide hidden items",
            "Save all editor files",
            "Collapse all folders",
            "Collapse All failed: {error}",
            "Save editor changes and finish imports before deleting files",
            "Explorer delete refused: {error}",
            "Explorer delete preflight refused: {error}",
            "Change one Explorer visibility option at a time",
            "Show/hide Explorer items",
            "Select a file or folder first",
            "Refresh Explorer",
        ];
        for source in labels {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [UiLocale::TraditionalChinese, UiLocale::SimplifiedChinese, UiLocale::Japanese] {
                assert_ne!(tr(locale, source), source, "Missing {source} / {}", locale.tag());
            }
        }
        for locale in UiLocale::ALL {
            for source in [
                "Confirm permanent deletion ({count})",
                "Explorer Paste succeeded: {count} item(s)",
                "Explorer operation succeeded: {action}{target}",
                "Explorer operation refused: {action}: {error}",
            ] {
                for placeholder in ["{count}", "{action}", "{target}", "{error}"] {
                    if source.contains(placeholder) {
                        assert!(tr(locale, source).contains(placeholder),
                            "{source}: missing {placeholder} in {}", locale.tag());
                    }
                }
            }
        }
        let dialog = include_str!("native_action_dialog_ui.rs");
        assert!(dialog.contains(".title(tr(locale, if action == ExplorerAction::Delete"));
        assert!(dialog.contains(".child(tr(locale, description))"));
        assert!(dialog.contains("tr(locale, \"Confirm permanent deletion ({count})\")"));
        assert!(dialog.contains("tr(locale, \"Cancel\")"));
        let workbench = include_str!("main.rs");
        assert!(workbench.contains("PopupMenuItem::new(tr(locale, if ignored"));
        assert!(workbench.contains("PopupMenuItem::new(tr(locale, if hidden"));
        assert!(workbench.contains("Explorer operation succeeded: {action}{target}"));
    }

    #[test]
    fn workbench_tooltips_and_explorer_empty_states_are_translated() {
        // Every visible Tab action and Explorer empty state must match the
        // saved language, not only right-click menu entries.
        let labels = [
            "New Tab",
            "New Tab in second pane",
            "Pane actions",
            "Second pane actions",
            "Close tab",
            "Close preview",
            "Close immutable Git original",
            "Scroll terminal history up one page (Shift+PageUp)",
            "Scroll terminal history down one page (Shift+PageDown)",
            "Scroll second-pane terminal history up (Shift+PageUp)",
            "Scroll second-pane terminal history down (Shift+PageDown)",
            "Indexing workspace files…",
            "No matching files or folders",
            "Tree limited to 4,000 visible items",
            "Move {count} Explorer items",
            "No Host terminal selected. Choose an existing session in Projects on the left.",
        ];
        for source in labels {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                assert_ne!(tr(locale, source), source, "{source} / {}", locale.tag());
                if source.contains("{count}") {
                    assert!(tr(locale, source).contains("{count}"));
                }
            }
        }
        let main = include_str!("main.rs");
        assert!(main.contains("tab_close_button((close_id, index), tr(locale, \"Close tab\"))"));
        assert!(main.contains(".child(tr(self.settings_menu_locale, if index_loading"));
        assert!(main.contains("\"No matching files or folders\""));
        assert!(main.contains("tr(self.settings_menu_locale, \"Move {count} Explorer items\")"));
    }

    #[test]
    fn new_tab_menu_actions_have_translations_in_every_supported_language() {
        let sources = [
            "Pane no longer available; reopen New Tab",
            "New File",
            "Pane changed; reopen New Tab",
            "New Terminal",
            "Installed Agent CLIs",
            "Detecting installed Agent CLIs…",
            "No installed Agent CLIs detected",
            "Rescan installed agents",
            "Host Agent Profiles",
            "New Agent",
            "Pending Host requests",
            "Recheck pending Host session (no new PTY)",
            "Host create result unknown · inspect running sessions before dismissing",
            "Cancel pending request (keep Host PTY)",
            "Attach running Host session",
            "No running Host sessions",
        ];
        for source in sources {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                let label = tr(locale, source);
                assert_ne!(label, source, "untranslated: {source} in {}", locale.tag());
                assert!(
                    !label.trim().is_empty(),
                    "empty: {source} in {}",
                    locale.tag()
                );
            }
        }
    }

    #[test]
    fn agent_state_chips_have_real_translations_in_all_four_locales() {
        for source in [
            "Agents",
            "Working",
            "Waiting for input",
            "Blocked",
            "Interrupted",
            "Done",
        ] {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                let label = tr(locale, source);
                assert_ne!(label, source, "{source} missing in {}", locale.tag());
                assert!(!label.trim().is_empty());
            }
        }
    }

    #[test]
    fn locale_variants_and_unknown_sources_fall_back_safely() {
        assert_eq!(
            UiLocale::from_preference("traditionalChinese"),
            UiLocale::TraditionalChinese
        );
        assert_eq!(UiLocale::from_preference("japanese"), UiLocale::Japanese);
        assert_eq!(UiLocale::from_preference("unknown"), UiLocale::English);
        assert_eq!(
            UiLocale::from_tag("zh_Hant_TW"),
            UiLocale::TraditionalChinese
        );
        assert_eq!(
            UiLocale::from_tag("zh_CN.UTF-8"),
            UiLocale::SimplifiedChinese
        );
        assert_eq!(UiLocale::from_tag("ja_JP"), UiLocale::Japanese);
        assert_eq!(UiLocale::from_tag("de-DE"), UiLocale::English);
        assert_eq!(
            tr(UiLocale::TraditionalChinese, "Alera-unknown-key-927"),
            "Alera-unknown-key-927"
        );
    }

    #[test]
    fn classic_right_sidebar_primary_controls_translate_in_every_locale() {
        for source in [
            "Source Control",
            "Commit",
            "Git History",
            "Open Git Graph",
            "Git Diff Preview",
            "Branch & remote actions",
            "No changes",
            "No matching files",
            "Git diff preview · read-only",
        ] {
            assert_eq!(tr(UiLocale::English, source), source);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                let localized = tr(locale, source);
                assert_ne!(localized, source, "Missing {source} in {}", locale.tag());
                assert!(!localized.trim().is_empty());
            }
        }
    }
    #[test]
    fn right_git_file_folder_context_actions_have_four_locales() {
        for label in [
            "Open with Default Application",
            "Preview changes",
            "Open worktree file in Alera",
            "Open worktree file externally",
            "Copy worktree file(s)",
            "Cut worktree file(s)",
            "Paste into containing folder",
            "Reveal worktree file in file manager",
            "Open Git original in read-only tab (HEAD / Index)",
            "View File History",
            "Add to .gitignore",
            "Stage selected",
            "Unstage selected",
            "Move untracked file(s) to Recycle Bin (confirm)",
            "Confirm Discard unstaged changes",
            "Discard unstaged changes (confirm)",
            "Copy worktree folder / selected items",
            "Cut worktree folder / selected items",
            "Paste into this folder",
            "Reveal folder in file manager",
            "Copy relative folder path",
        ] {
            assert_eq!(tr(UiLocale::English, label), label);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                let localized = tr(locale, label);
                assert_ne!(localized, label, "{label} missing in {}", locale.tag());
                assert!(!localized.trim().is_empty());
            }
        }
    }
    #[test]
    fn git_file_revision_compare_actions_have_four_locales() {
        for label in [
            "Compare With Latest Git Revision",
            "Compare With Branch...",
            "Compare With Branch",
        ] {
            assert_eq!(tr(UiLocale::English, label), label);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                assert_ne!(tr(locale, label), label, "{label} missing in {}", locale.tag());
            }
        }
    }
    #[test]
    fn git_file_compare_layout_and_content_controls_have_four_locales() {
        for label in [
            "Full file",
            "Differences only",
            "Side-by-side view",
            "Unified view",
            "Git file comparison",
            "Pin comparison in workbench",
        ] {
            assert_eq!(tr(UiLocale::English, label), label);
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                assert_ne!(tr(locale, label), label, "{label} missing in {}", locale.tag());
            }
        }
    }
}
