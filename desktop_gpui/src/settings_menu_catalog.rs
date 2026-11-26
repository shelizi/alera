//! Classic Settings navigation and visual-only row catalog.
//!
//! The labels and grouping mirror lib/src/features/settings/presentation/
//! settings_dialog.dart. This is a UI preview: it contains no settings owner,
//! persistence, runtime, or provider callbacks.
use crate::native_i18n::{tr, UiLocale};

/// Same values and native-language labels as Classic's App Language dropdown.
/// These are display choices only; every write still goes through Rust Host CAS.
pub const APP_LANGUAGE_CHOICES: [(&str, &str); 5] = [
    ("system", "Follow System"),
    ("english", "English"),
    ("traditionalChinese", "繁體中文"),
    ("simplifiedChinese", "简体中文"),
    ("japanese", "日本語"),
];

/// Exact theme names and order from Classic's terminal theme catalog.
pub const TERMINAL_THEME_CHOICES: &[&str] = &[
    "Alera Dark",
    "Ghostty Default Style Dark",
    "Builtin Tango Light",
    "Dracula",
    "One Dark",
    "Nord",
    "Monokai",
    "Tokyo Night",
    "Gruvbox Dark",
    "Catppuccin Mocha",
    "Solarized Dark",
    "Material Dark",
    "Ayu Dark",
    "Nightfox",
    "Kanagawa",
    "Rose Pine",
    "Everforest Dark",
    "Palenight",
    "Horizon Dark",
    "Night Owl",
    "Solarized Light",
    "One Light",
    "Catppuccin Latte",
    "GitHub Light",
    "Rose Pine Dawn",
    "Gruvbox Light",
    "Tokyo Night Light",
    "Everforest Light",
    "Tango Dark",
    "Homebrew",
    "Snazzy",
];

/// Exact theme names and order from Classic's syntax theme catalog.
pub const EDITOR_SYNTAX_THEME_CHOICES: &[&str] = &[
    "Alera",
    "GitHub Dark",
    "GitHub Dark Dimmed",
    "VS2015",
    "Atom One Dark",
    "Night Owl",
    "Nord",
    "Monokai",
    "Tokyo Night Dark",
    "Dracula",
];

/// Filter the Classic theme catalog without changing its authored order.
/// Case-insensitive words can match anywhere in the name; missing results
/// never rewrite or replace the authoritative Rust Host preference.
pub fn matching_theme_choices<'a>(choices: &'a [&'a str], query: &str) -> Vec<&'a str> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    choices
        .iter()
        .copied()
        .filter(|name| {
            let lower = name.to_lowercase();
            words.iter().all(|word| lower.contains(word))
        })
        .collect()
}

/// Classic-style installed font suggestions: prefix matches first, then
/// substring matches. Never restrict a typed name to this suggestion list.
pub fn matching_font_suggestions<'a>(
    installed: &'a [String],
    query: &str,
    max_results: usize,
) -> Vec<&'a str> {
    let query = query.trim().to_lowercase();
    if query.is_empty() || max_results == 0 {
        return Vec::new();
    }
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in installed {
        let lower = name.to_lowercase();
        if !seen.insert(lower.clone()) {
            continue;
        }
        if lower.starts_with(&query) {
            starts.push(name.as_str());
        } else if lower.contains(&query) {
            contains.push(name.as_str());
        }
    }
    starts
        .into_iter()
        .chain(contains)
        .take(max_results)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsNavGroup {
    Preferences,
    Resources,
}

impl SettingsNavGroup {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Preferences => "Preferences",
            Self::Resources => "Resources",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewControl {
    Toggle,
    Select,
    Input,
    Action,
}

#[derive(Debug, Clone, Copy)]
pub struct PreviewRow {
    pub title: &'static str,
    pub detail: &'static str,
    pub control: PreviewControl,
}

#[derive(Debug, Clone, Copy)]
pub struct PreviewGroup {
    pub title: &'static str,
    pub rows: &'static [PreviewRow],
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsSection {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub nav_group: SettingsNavGroup,
    pub groups: &'static [PreviewGroup],
    /// Classic shows heading chips only for sections with 3+ declared groups.
    pub show_group_chips: bool,
}

macro_rules! row {
    ($title:literal, $detail:literal, $kind:ident) => {
        PreviewRow {
            title: $title,
            detail: $detail,
            control: PreviewControl::$kind,
        }
    };
}
macro_rules! group {
    ($title:literal => [$($row:expr),* $(,)?]) => {
        PreviewGroup { title: $title, rows: &[$($row),*] }
    };
}

pub const SECTIONS: &[SettingsSection] = &[
    SettingsSection {
        id: "configurationSync", title: "Configuration Sync", icon: "↻",
        description: "Review, download and upload configuration across your devices.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: false,
        groups: &[group!("Configuration Sync" => [
            row!("Configuration Sync", "Review configuration across devices.", Action),
            row!("Upload configuration", "Upload and merge configuration.", Action),
        ])],
    },
    SettingsSection {
        id: "account", title: "Account", icon: "◉",
        description: "Identity, mobile push and runtime ownership.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Identity" => [row!("Account", "Manage identity for this device.", Action)]),
            group!("Mobile Push" => [row!("Mobile Push", "Notification configuration.", Toggle)]),
            group!("Ownership" => [row!("Runtime Ownership", "Manage registered runtime owners.", Action)]),
        ],
    },
    SettingsSection {
        id: "application", title: "Application", icon: "⚙",
        description: "Storage, safety, runtime, diagnostics and updates.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Language" => [row!("App Language", "Follow the system language or choose a language for Alera.", Select)]),
            group!("Storage" => [row!("Workspace Directory", "Where new linked workspaces are created on disk.", Input)]),
            group!("Workspaces" => [
                row!("Automatically Fetch Git Remotes", "Periodically fetch remote branch information.", Toggle),
                row!("Auto Git Fetch Interval", "Minutes between automatic Git fetches.", Select),
                row!("Auto-Archive After Inactivity", "Move inactive workspaces into Archived.", Select),
            ]),
            group!("Safety" => [
                row!("Confirm Project Removal", "Ask before unregistering a project.", Toggle),
                row!("Confirm Workspace Removal", "Confirmation for worktree removal.", Toggle),
            ]),
            group!("Desktop" => [
                row!("Show Tray Icon", "Keep Alera in the notification area.", Toggle),
                row!("Show Dock Badge", "Show the number of agents waiting for review.", Toggle),
                row!("Show Tray Badge", "Show agent review count on the tray icon.", Toggle),
            ]),
            group!("Pull Requests" => [
                row!("Show Pull Request Status", "Show review and CI state in the sidebar.", Toggle),
                row!("Notify When Checks Fail", "Show a native notification for failed checks.", Toggle),
            ]),
            group!("Runtime" => [
                row!("Keep Computer Awake", "Prevent idle sleep while Alera is running.", Toggle),
                row!("Keep Runtime Open When App Quits", "Leave the runtime sidecar running.", Toggle),
            ]),
            group!("Diagnostics" => [
                row!("Log Level", "How much detail is written to log files.", Select),
                row!("Export Diagnostics", "Save diagnostics as a zip.", Action),
            ]),
            group!("Updates" => [row!("Updates", "Check releases for this platform.", Action)]),
            group!("Support" => [row!("Star Alera on GitHub", "Show your support for the project.", Action)]),
        ],
    },
    SettingsSection {
        id: "agents", title: "Agents", icon: "◇",
        description: "Agent hooks, notifications and Alera skills.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("CLI And Skills" => [row!("All Alera Skills", "Install or update core agent skills.", Action), row!("Alera CLI Skill", "Agent instructions for the CLI.", Action)]),
            group!("Extra Skills" => [row!("Agent Profiles Skill", "Catalog and managed-profile instructions.", Action)]),
            group!("Status Hooks" => [row!("Codex Hooks", "Managed status hooks.", Toggle), row!("Claude Code Hooks", "Managed status hooks.", Toggle)]),
            group!("Behavior" => [
                row!("Built-in Alera MCP", "Register alera-workbench for new Managed Codex and Claude sessions. Existing sessions must be restarted; other providers are not supported.", Toggle),
                row!("Show Tab Titles in Sidebar", "Display agent tab titles in the project tree.", Toggle),
                row!("Show Agent Task Summaries in Sidebar", "Show the overall task and recent report in multiline Agent rows (off by default).", Toggle),
                row!("Agent Status Notifications", "Notify when an agent needs attention.", Toggle),
                row!("Agent Finished Notifications", "Also notify when an agent reports completion.", Toggle),
            ]),
        ],
    },
    SettingsSection {
        id: "quotas", title: "Quotas", icon: "◎",
        description: "Provider usage, Claude profiles and credential environment.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Providers" => [row!("Provider Quotas", "Usage and availability by provider.", Action)]),
            group!("Claude" => [row!("Claude Profiles", "Configure quota profile display.", Action)]),
            group!("Credentials" => [row!("Credential Environment", "Read-only credential environment status.", Action)]),
        ],
    },
    SettingsSection {
        id: "aiAssist", title: "AI Assist", icon: "✧",
        description: "Local agent assistance for commits, pull requests, diffs, workspace identity, and speech.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Generation" => [row!("Generation", "Local assistance defaults.", Select)]),
            group!("Commit Messages" => [row!("Commit Messages", "Assist with Git commit messages.", Toggle)]),
            group!("Merge Conflicts" => [row!("Merge Conflicts", "Assistance during conflict resolution.", Toggle)]),
            group!("Pull Request Details" => [row!("Pull Request Details", "Compose PR descriptions.", Toggle)]),
            group!("Reading Diffs" => [row!("Reading Diffs", "Summarize changes.", Toggle)]),
            group!("Workspace Identity" => [row!("Workspace Identity", "Context used by local assistance.", Select)]),
        ],
    },
    SettingsSection {
        id: "aiDictation", title: "AI Dictation", icon: "♬",
        description: "Local, Codex subscription, and OpenAI-compatible speech-to-text.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Transcription" => [row!("Transcription", "Speech-to-text provider.", Select)]),
            group!("Remote Transcription" => [row!("Remote Transcription", "Remote transcription options.", Select)]),
            group!("Local Whisper Models" => [row!("Local Whisper Models", "Model management.", Action)]),
            group!("Speech Processing" => [row!("Speech Processing", "Audio processing defaults.", Toggle)]),
            group!("Test AI Dictation" => [row!("Test AI Dictation", "Preview a microphone test.", Action)]),
        ],
    },
    SettingsSection {
        id: "textActions", title: "Text Actions", icon: "≡",
        description: "Create reusable replacements for selected text.",
        nav_group: SettingsNavGroup::Resources, show_group_chips: false,
        groups: &[group!("Actions" => [row!("Text Actions", "Create or edit reusable text actions.", Action)])],
    },
    SettingsSection {
        id: "editor", title: "Editor", icon: "<>",
        description: "Code editor defaults.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Tabs" => [row!("Two-row tabs", "Pinned tabs on first row, regular tabs below.", Toggle)]),
            group!("Appearance" => [row!("Syntax Theme", "Syntax highlighting defaults.", Select)]),
            group!("Indentation" => [row!("Tab Size", "Spaces inserted when pressing Tab.", Select)]),
            group!("Autosave" => [row!("Autosave", "Save dirty editor tabs after they are idle.", Toggle), row!("Autosave Delay", "Time before saving an idle buffer.", Select)]),
            group!("Quick Open" => [row!("Excluded Directories", "Ignore generated and dependency folders.", Input)]),
            group!("Language Intelligence" => [row!("Language Intelligence", "Code intelligence provider settings.", Select)]),
            group!("External Editor" => [
                row!("External Editor", "Editor used by Open In menu entries, keyboard shortcuts, and external file targets.", Select),
                row!("Default Code Open Target", "Choose where normal editable source and text files open. Dedicated Alera previews stay internal.", Select),
                row!("Custom Executable", "Optional per-device executable override.", Input),
                row!("Open Workspaces in New Window", "Choose how external workspaces open.", Toggle),
                row!("Auto-open New Workspaces Externally", "Launch newly created workspaces in the external editor.", Toggle),
                row!("Check External Editor", "Run a non-destructive version check with the current executable setting.", Action),
            ]),
        ],
    },
    SettingsSection {
        id: "terminal", title: "Terminal", icon: ">_",
        description: "Appearance defaults for new terminal sessions.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: true,
        groups: &[
            group!("Typography" => [row!("Font Family", "Typeface used in new terminal sessions.", Select), row!("Font Size", "Text size for new sessions.", Select), row!("Line Height", "Vertical spacing for terminal rows.", Select)]),
            group!("Cursor" => [row!("Cursor Shape", "Caret style in new sessions.", Select), row!("Blinking Cursor", "Blink the cursor while focused.", Toggle)]),
            group!("Appearance" => [row!("Theme Preset", "Built-in terminal colors.", Select), row!("Background Opacity", "Opacity of terminal background.", Select)]),
            group!("Interaction" => [row!("TUI Scroll Speed", "Mouse wheel speed for terminal applications.", Select), row!("Copy On Select", "Copy selections automatically.", Toggle)]),
            group!("Advanced" => [row!("PowerShell 7 Executable", "Windows pwsh.exe path.", Input), row!("Scrollback Lines", "Maximum terminal history.", Select)]),
        ],
    },
    SettingsSection {
        id: "keyboard", title: "Keyboard", icon: "⌘",
        description: "Shortcuts and key bindings.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: false,
        groups: &[group!("Keyboard Shortcuts" => [row!("Search shortcuts", "Find command bindings.", Input), row!("Reset Keyboard Shortcuts", "Restore default bindings.", Action)])],
    },
    SettingsSection {
        id: "projects", title: "Projects", icon: "▣",
        description: "Per-project workspace setup.",
        nav_group: SettingsNavGroup::Resources, show_group_chips: false,
        groups: &[group!("Projects" => [row!("Projects", "Select a project to view workspace options.", Action), row!("Project Settings", "Per-project configuration.", Input)])],
    },
    SettingsSection {
        id: "mobileDevices", title: "Mobile Devices", icon: "▯",
        description: "Pair and manage the mobile companion app.",
        nav_group: SettingsNavGroup::Resources, show_group_chips: true,
        groups: &[
            group!("Mobile Gateway" => [row!("Mobile Gateway", "Connect mobile companions.", Toggle)]),
            group!("Link A Device" => [row!("Link A Device", "Show the pairing flow.", Action)]),
            group!("Active Pairing Offers" => [row!("Active Pairing Offers", "Review pending offers.", Action)]),
            group!("Paired Devices" => [row!("Paired Devices", "Review linked devices.", Action)]),
        ],
    },
    SettingsSection {
        id: "remoteHosts", title: "Remote Hosts", icon: "▤",
        description: "SSH runtime targets.",
        nav_group: SettingsNavGroup::Resources, show_group_chips: false,
        groups: &[group!("Remote Hosts" => [row!("Remote Hosts", "Manage SSH runtime targets.", Action)])],
    },
    SettingsSection {
        id: "agentProfiles", title: "Agent Profiles", icon: "◇",
        description: "Launch configurations orchestration can dispatch to.",
        nav_group: SettingsNavGroup::Resources, show_group_chips: false,
        groups: &[group!("Agent Profiles" => [row!("Agent Profiles", "Configure available launch profiles.", Action)])],
    },
    SettingsSection {
        id: "historySync", title: "History Sync", icon: "↻",
        description: "Share Agent conversation history across account groups through Runtime Host.",
        nav_group: SettingsNavGroup::Preferences, show_group_chips: false,
        groups: &[],
    },
];

pub fn section(id: &str) -> Option<&'static SettingsSection> {
    SECTIONS.iter().find(|section| section.id == id)
}

/// Classic's visible row labels mapped to validated Rust-owned field paths.
/// Unmapped actions remain visibly unsupported rather than fake switches.
pub fn shared_setting_row_path(section: &str, title: &str) -> Option<&'static str> {
    Some(match (section, title) {
        ("application", "Automatically Fetch Git Remotes") => "general.autoGitFetchEnabled",
        ("application", "Auto Git Fetch Interval") => "general.autoGitFetchIntervalMinutes",
        ("application", "Show Tray Icon") => "general.showTrayIcon",
        ("application", "Show Dock Badge") => "general.showDockBadge",
        ("application", "Show Tray Badge") => "general.showTrayBadge",
        ("editor", "Two-row tabs") => "editor.twoRowTabs",
        ("editor", "External Editor") => "editor.externalEditor",
        ("terminal", "Font Family") => "terminal.fontFamily",
        ("terminal", "Font Size") => "terminal.fontSize",
        ("terminal", "Line Height") => "terminal.lineHeight",
        ("terminal", "Cursor Shape") => "terminal.cursorShape",
        ("terminal", "Blinking Cursor") => "terminal.cursorBlink",
        ("terminal", "Theme Preset") => "terminal.themeName",
        ("terminal", "Background Opacity") => "terminal.backgroundOpacity",
        ("terminal", "TUI Scroll Speed") => "terminal.tuiScrollSensitivity",
        ("terminal", "Copy On Select") => "terminal.clipboardOnSelect",
        ("editor", "Syntax Theme") => "editor.themeName",
        ("editor", "Tab Size") => "editor.tabSize",
        ("editor", "Autosave") => "editor.autosaveEnabled",
        ("editor", "Autosave Delay") => "editor.autosaveDelaySeconds",
        ("editor", "Excluded Directories") => "editor.quickOpenExcludedDirectories",
        ("agents", "Built-in Alera MCP") => "agents.builtinMcpEnabled",
        ("agents", "Show Tab Titles in Sidebar") => "agents.showTabTitlesInSidebar",
        ("agents", "Show Agent Task Summaries in Sidebar") => "agents.showTaskSummariesInSidebar",
        ("agents", "Agent Status Notifications") => "agents.agentStatusNotificationsEnabled",
        ("agents", "Agent Finished Notifications") => {
            "agents.agentStatusFinishedNotificationsEnabled"
        }
        _ => return None,
    })
}

#[cfg(test)]
mod shared_row_tests {
    use super::*;
    #[test]
    fn s01_every_visible_classic_row_has_one_grounded_capability() {
        let mut totals = SettingsSupportCounts::default();
        for section in SECTIONS {
            let current = settings_support_counts(section);
            assert_eq!(
                current.total(),
                section
                    .groups
                    .iter()
                    .map(|group| group.rows.len())
                    .sum::<usize>(),
                "S01 classification must cover every visible row in {}",
                section.id
            );
            totals.editable += current.editable;
            totals.actions += current.actions;
            totals.read_only += current.read_only;
            totals.unavailable += current.unavailable;
        }
        assert_eq!(SECTIONS.len(), 16);
        assert!(
            totals.editable >= 20,
            "Host-backed controls unexpectedly missing"
        );
        assert_eq!(
            totals.actions, 6,
            "exactly the six connected Native action handlers may be clickable"
        );
        assert_eq!(
            totals.read_only, 3,
            "safety rows must not gain write controls"
        );
        assert!(totals.unavailable > 0, "Classic-only rows must be honest");
        assert_eq!(
            settings_support_counts(section("editor").unwrap()).editable,
            7,
            "Two-row tabs, editor choice, and five existing editor controls must be wired"
        );
        assert_eq!(
            settings_support_counts(section("quotas").unwrap()).actions,
            1,
            "Provider Quotas must open the working overview"
        );
    }
    #[test]
    fn all_connected_rows_resolve_to_the_one_rust_core_allowlist() {
        for section in SECTIONS {
            for group in section.groups {
                for row in group.rows {
                    if let Some(path) = shared_setting_row_path(section.id, row.title) {
                        assert!(
                            alera_core::desktop_settings_contract::cli_writable(path),
                            "{path} has no approved Rust Core writer"
                        );
                    }
                }
            }
        }
        assert_eq!(
            settings_support_counts(section("keyboard").unwrap()).actions,
            1
        );
        assert_eq!(
            settings_support_counts(section("agentProfiles").unwrap()).actions,
            1
        );
        assert_eq!(
            settings_row_support("keyboard", "Reset Keyboard Shortcuts"),
            SettingsRowSupport::Unavailable,
            "Reset cannot claim capability until overrides exist"
        );
        assert_eq!(
            shared_setting_row_path("editor", "Two-row tabs"),
            Some("editor.twoRowTabs"),
            "Classic Editor tab layout must write the Host-backed shared setting"
        );
        assert_eq!(
            settings_row_support("quotas", "Provider Quotas"),
            SettingsRowSupport::OperationalAction
        );
        assert_eq!(
            settings_operational_action("quotas", "Provider Quotas"),
            Some(SettingsOperationalAction::OpenQuotaOverview)
        );
        assert_eq!(
            settings_operational_action("projects", "Projects"),
            Some(SettingsOperationalAction::ManageProjects)
        );
        assert_eq!(
            settings_row_support("application", "Confirm Project Removal"),
            SettingsRowSupport::ReadOnly
        );
        assert_eq!(
            settings_row_support("application", "Confirm Workspace Removal"),
            SettingsRowSupport::ReadOnly
        );
        assert_eq!(
            settings_row_support("application", "Auto-Archive After Inactivity"),
            SettingsRowSupport::ReadOnly
        );
        assert_eq!(
            settings_operational_action("terminal", "Scrollback Lines"),
            Some(SettingsOperationalAction::ConfigureNativeScrollback)
        );
        assert_eq!(
            settings_row_support("account", "Mobile Push"),
            SettingsRowSupport::Unavailable
        );
        assert_eq!(
            settings_row_support("editor", "Two-row tabs"),
            SettingsRowSupport::Editable
        );
        assert_eq!(
            shared_setting_row_path("terminal", "Cursor Shape"),
            Some("terminal.cursorShape")
        );
        assert_eq!(
            shared_setting_row_path("editor", "External Editor"),
            Some("editor.externalEditor")
        );
        for section in SECTIONS {
            let counts = settings_support_counts(section);
            let all_rows: usize = section.groups.iter().map(|group| group.rows.len()).sum();
            assert_eq!(
                counts.total(),
                all_rows,
                "incomplete capability audit in {}",
                section.id
            );
            for group in section.groups {
                for row in group.rows {
                    let status = settings_row_support(section.id, row.title);
                    if let SettingsRowSupport::Editable = status {
                        assert!(
                            shared_setting_row_path(section.id, row.title)
                                .is_some_and(alera_core::desktop_settings_contract::cli_writable)
                                || matches!(
                                    (section.id, row.title),
                                    ("application", "App Language")
                                        | ("agents", "Codex Hooks")
                                        | ("agents", "Claude Code Hooks")
                                ),
                            "Catalog row is marked editable with no authorized handler: {} / {}",
                            section.id,
                            row.title,
                        );
                    }
                }
            }
        }
        assert_eq!(
            shared_setting_row_path("aiDictation", "Test AI Dictation"),
            None
        );
    }
}

/// S01 capability audit, separate from dynamic Runtime Host availability.
/// Host-backed rows still require the authoritative snapshot before editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRowSupport {
    Editable,
    OperationalAction,
    ReadOnly,
    Unavailable,
}

pub fn settings_row_support(section: &str, title: &str) -> SettingsRowSupport {
    if shared_setting_row_path(section, title).is_some()
        || matches!(
            (section, title),
            ("application", "App Language")
                | ("agents", "Codex Hooks")
                | ("agents", "Claude Code Hooks")
        )
    {
        return SettingsRowSupport::Editable;
    }
    if settings_operational_action(section, title).is_some() {
        return SettingsRowSupport::OperationalAction;
    }
    match (section, title) {
        ("application", "Confirm Project Removal")
        | ("application", "Confirm Workspace Removal")
        | ("application", "Auto-Archive After Inactivity") => SettingsRowSupport::ReadOnly,
        _ => SettingsRowSupport::Unavailable,
    }
}

/// Only actions with actual Native UI handlers may claim operational support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsOperationalAction {
    ManageProjects,
    ViewHostProjectSettings,
    OpenQuotaOverview,
    SearchNativeKeyboardShortcuts,
    RefreshHostAgentProfiles,
    ConfigureNativeScrollback,
}

pub fn settings_operational_action(
    section: &str,
    title: &str,
) -> Option<SettingsOperationalAction> {
    match (section, title) {
        ("projects", "Projects") => Some(SettingsOperationalAction::ManageProjects),
        ("projects", "Project Settings") => {
            Some(SettingsOperationalAction::ViewHostProjectSettings)
        }
        ("quotas", "Provider Quotas") => Some(SettingsOperationalAction::OpenQuotaOverview),
        ("keyboard", "Search shortcuts") => {
            Some(SettingsOperationalAction::SearchNativeKeyboardShortcuts)
        }
        ("agentProfiles", "Agent Profiles") => {
            Some(SettingsOperationalAction::RefreshHostAgentProfiles)
        }
        ("terminal", "Scrollback Lines") => {
            Some(SettingsOperationalAction::ConfigureNativeScrollback)
        }
        _ => None,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SettingsSupportCounts {
    pub editable: usize,
    pub actions: usize,
    pub read_only: usize,
    pub unavailable: usize,
}

impl SettingsSupportCounts {
    pub fn total(self) -> usize {
        self.editable + self.actions + self.read_only + self.unavailable
    }
}

pub fn settings_support_counts(section: &SettingsSection) -> SettingsSupportCounts {
    let mut counts = SettingsSupportCounts::default();
    for group in section.groups {
        for row in group.rows {
            match settings_row_support(section.id, row.title) {
                SettingsRowSupport::Editable => counts.editable += 1,
                SettingsRowSupport::OperationalAction => counts.actions += 1,
                SettingsRowSupport::ReadOnly => counts.read_only += 1,
                SettingsRowSupport::Unavailable => counts.unavailable += 1,
            }
        }
    }
    counts
}

/// Only five sections existed as connected Native controls before parity.
/// Other Classic sections are visual previews and cannot write settings.
pub fn has_connected_native_controls(id: &str) -> bool {
    matches!(
        id,
        "application" | "terminal" | "editor" | "agents" | "agentProfiles"
    )
}

pub fn matching_sections(query: &str) -> Vec<&'static SettingsSection> {
    let query = query.trim().to_lowercase();
    SECTIONS
        .iter()
        .filter(|section| {
            query.is_empty()
                || section.title.to_lowercase().contains(&query)
                || section.description.to_lowercase().contains(&query)
                || section.groups.iter().any(|group| {
                    group.title.to_lowercase().contains(&query)
                        || group.rows.iter().any(|row| {
                            row.title.to_lowercase().contains(&query)
                                || row.detail.to_lowercase().contains(&query)
                        })
                })
        })
        .collect()
}

/// Search translated text and the English original, without touching preferences.
pub fn matching_sections_localized(query: &str, locale: UiLocale) -> Vec<&'static SettingsSection> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return SECTIONS.iter().collect();
    }
    let hit = |value: &str| {
        value.to_lowercase().contains(&query) || tr(locale, value).to_lowercase().contains(&query)
    };
    SECTIONS
        .iter()
        .filter(|section| {
            hit(section.title)
                || hit(section.description)
                || section.groups.iter().any(|group| {
                    hit(group.title)
                        || group
                            .rows
                            .iter()
                            .any(|row| hit(row.title) || hit(row.detail))
                })
        })
        .collect()
}

/// Classic's sidebar badge counts matching setting entries, not a matching
/// section title. Keep this independent of the connected Host capabilities.
pub fn matching_row_count_localized(
    section: &SettingsSection,
    query: &str,
    locale: UiLocale,
) -> usize {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }
    let matches = |value: &str| {
        value.to_lowercase().contains(&query) || tr(locale, value).to_lowercase().contains(&query)
    };
    section
        .groups
        .iter()
        .flat_map(|group| group.rows)
        .filter(|row| matches(row.title) || matches(row.detail))
        .count()
}

/// The settings body paints its capability summary first, then one direct
/// scroll child per group. Callers add one to this index when scrolling.
pub fn first_matching_group_index_localized(
    section: &SettingsSection,
    query: &str,
    locale: UiLocale,
) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }
    let matches = |value: &str| {
        value.to_lowercase().contains(&query) || tr(locale, value).to_lowercase().contains(&query)
    };
    section.groups.iter().position(|group| {
        matches(group.title)
            || group
                .rows
                .iter()
                .any(|row| matches(row.title) || matches(row.detail))
    })
}

pub fn modal_size(viewport_width: f32, viewport_height: f32) -> (f32, f32, bool) {
    let width = (viewport_width.max(0.0) * 0.92)
        .min(1800.0)
        .min((viewport_width - 16.0).max(0.0));
    let height = (viewport_height.max(0.0) * 0.92)
        .min(1280.0)
        .min((viewport_height - 16.0).max(0.0));
    (width, height, width < 720.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_theme_picker_search_keeps_order_and_filters_case_insensitively() {
        assert_eq!(
            matching_theme_choices(EDITOR_SYNTAX_THEME_CHOICES, "gitHUB"),
            vec!["GitHub Dark", "GitHub Dark Dimmed"]
        );
        assert_eq!(
            matching_theme_choices(EDITOR_SYNTAX_THEME_CHOICES, "dark dimmed"),
            vec!["GitHub Dark Dimmed"]
        );
        assert_eq!(
            matching_theme_choices(TERMINAL_THEME_CHOICES, "light"),
            TERMINAL_THEME_CHOICES
                .iter()
                .copied()
                .filter(|name| name.contains("Light"))
                .collect::<Vec<_>>()
        );
        assert!(matching_theme_choices(EDITOR_SYNTAX_THEME_CHOICES, "no-such-theme").is_empty());
        assert_eq!(
            matching_theme_choices(EDITOR_SYNTAX_THEME_CHOICES, "  "),
            EDITOR_SYNTAX_THEME_CHOICES.to_vec()
        );
    }

    #[test]
    fn classic_sections_and_sidebar_order_are_preserved() {
        let ids: Vec<_> = SECTIONS.iter().map(|section| section.id).collect();
        assert_eq!(
            ids,
            vec![
                "configurationSync",
                "account",
                "application",
                "agents",
                "quotas",
                "aiAssist",
                "aiDictation",
                "textActions",
                "editor",
                "terminal",
                "keyboard",
                "projects",
                "mobileDevices",
                "remoteHosts",
                "agentProfiles",
                "historySync"
            ]
        );
        assert_eq!(
            SECTIONS
                .iter()
                .filter(|s| s.nav_group == SettingsNavGroup::Resources)
                .count(),
            5
        );
        assert_eq!(section("application").unwrap().groups.len(), 10);
        assert_eq!(section("terminal").unwrap().groups.len(), 5);
    }

    #[test]
    fn group_chips_follow_classic_three_group_threshold() {
        for section in SECTIONS {
            assert_eq!(
                section.show_group_chips,
                section.groups.len() >= 3,
                "Classic renders group chips for every section with at least three groups: {}",
                section.id
            );
        }
        assert!(section("editor").unwrap().show_group_chips);
        assert!(section("terminal").unwrap().show_group_chips);
    }

    #[test]
    fn only_existing_native_settings_sections_are_connected() {
        let connected = SECTIONS
            .iter()
            .filter(|entry| has_connected_native_controls(entry.id))
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        assert_eq!(
            connected,
            vec![
                "application",
                "agents",
                "editor",
                "terminal",
                "agentProfiles"
            ]
        );
        assert!(!has_connected_native_controls("configurationSync"));
        assert!(!has_connected_native_controls("account"));
        assert!(!has_connected_native_controls("remoteHosts"));
        assert!(section("not-a-settings-id").is_none());
    }

    #[test]
    fn local_search_filters_groups_and_rows_without_backends() {
        assert_eq!(matching_sections("").len(), SECTIONS.len());
        assert!(matching_sections("Font Family")
            .iter()
            .any(|s| s.id == "terminal"));
        assert!(matching_sections("mobile gateway")
            .iter()
            .any(|s| s.id == "mobileDevices"));
        assert!(matching_sections("no-existent-section-xyz").is_empty());
    }

    #[test]
    fn localized_search_handles_chinese_and_japanese_without_losing_english() {
        let zh = matching_sections_localized("終端機", UiLocale::TraditionalChinese);
        assert!(zh.iter().any(|section| section.id == "terminal"));
        let zh_cn = matching_sections_localized("工作区", UiLocale::SimplifiedChinese);
        assert!(zh_cn.iter().any(|section| section.id == "application"));
        let ja = matching_sections_localized("ターミナル", UiLocale::Japanese);
        assert!(ja.iter().any(|section| section.id == "terminal"));
        assert!(
            matching_sections_localized("Font Family", UiLocale::Japanese)
                .iter()
                .any(|section| section.id == "terminal")
        );
        assert!(matching_sections_localized("missing-query-827", UiLocale::Japanese).is_empty());
    }

    #[test]
    fn classic_search_badge_counts_rows_and_jumps_to_the_matching_group() {
        let editor = section("editor").unwrap();
        assert_eq!(
            matching_row_count_localized(editor, "", UiLocale::English),
            0
        );
        assert_eq!(
            matching_row_count_localized(editor, "Autosave", UiLocale::English),
            2
        );
        assert_eq!(
            first_matching_group_index_localized(editor, "Autosave Delay", UiLocale::English),
            Some(3)
        );
        assert_eq!(
            first_matching_group_index_localized(
                editor,
                "Code editor defaults.",
                UiLocale::English,
            ),
            None
        );
        assert_eq!(
            first_matching_group_index_localized(editor, "no-such-setting", UiLocale::English),
            None
        );
        let translated = tr(UiLocale::TraditionalChinese, "Autosave Delay");
        assert_eq!(
            first_matching_group_index_localized(editor, &translated, UiLocale::TraditionalChinese),
            Some(3)
        );
    }

    #[test]
    fn app_language_uses_classic_order_and_host_editable_route() {
        let application = section("application").unwrap();
        assert_eq!(application.groups[0].title, "Language");
        assert_eq!(application.groups[0].rows[0].title, "App Language");
        assert_eq!(
            settings_row_support("application", "App Language"),
            SettingsRowSupport::Editable
        );
        assert_eq!(
            APP_LANGUAGE_CHOICES,
            [
                ("system", "Follow System"),
                ("english", "English"),
                ("traditionalChinese", "繁體中文"),
                ("simplifiedChinese", "简体中文"),
                ("japanese", "日本語"),
            ]
        );
    }

    #[test]
    fn offline_settings_search_and_refresh_have_classic_translations() {
        for locale in [
            UiLocale::TraditionalChinese,
            UiLocale::SimplifiedChinese,
            UiLocale::Japanese,
        ] {
            assert_ne!(tr(locale, "Search settings"), "Search settings");
            assert_ne!(tr(locale, "Refresh"), "Refresh");
        }
    }

    #[test]
    fn installed_font_autocomplete_prioritizes_prefix_and_preserves_custom_input() {
        let installed = vec![
            "Cascadia Code".to_owned(),
            "Code New Roman".to_owned(),
            "Consolas".to_owned(),
            "JetBrains Mono".to_owned(),
            "cONsolas".to_owned(),
            "Other Code".to_owned(),
        ];
        assert_eq!(
            matching_font_suggestions(&installed, "co", 4),
            vec!["Code New Roman", "Consolas", "Cascadia Code", "Other Code"]
        );
        assert_eq!(
            matching_font_suggestions(&installed, "CON", 2),
            vec!["Consolas"]
        );
        assert!(matching_font_suggestions(&installed, "unknown custom font", 10).is_empty());
        assert!(matching_font_suggestions(&installed, "", 10).is_empty());
        assert!(matching_font_suggestions(&installed, "code", 0).is_empty());
    }

    #[test]
    fn theme_picker_choices_match_classic_catalogs_without_duplicates() {
        use std::collections::HashSet;

        let classic_terminal_catalog =
            include_str!("../../lib/src/features/settings/domain/terminal_theme_catalog.dart");
        let classic_terminal_base =
            include_str!("../../lib/src/features/settings/domain/terminal_theme_base_palette.dart");
        let classic_terminal_extended = include_str!(
            "../../lib/src/features/settings/domain/terminal_theme_extended_palette.dart"
        );
        let classic_editor_catalog =
            include_str!("../../lib/src/features/settings/domain/editor_syntax_theme_catalog.dart");

        let expected_terminal_count = 1
            + classic_terminal_base.matches("_hexTheme(").count()
            + classic_terminal_extended.matches("_hexTheme(").count();
        assert_eq!(
            TERMINAL_THEME_CHOICES.len(),
            expected_terminal_count,
            "Classic terminal theme catalog changed; update GPUI theme choices"
        );
        assert_eq!(
            EDITOR_SYNTAX_THEME_CHOICES.len(),
            classic_editor_catalog
                .matches("name: EditorSyntaxThemeNames.")
                .count(),
            "Classic editor theme catalog changed; update GPUI theme choices"
        );

        let terminal_sources =
            format!("{classic_terminal_catalog}{classic_terminal_base}{classic_terminal_extended}");
        let unique_terminal = TERMINAL_THEME_CHOICES
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        assert_eq!(unique_terminal.len(), TERMINAL_THEME_CHOICES.len());
        for name in TERMINAL_THEME_CHOICES {
            assert!(
                terminal_sources.contains(&format!("'{name}'")),
                "GPUI terminal theme missing from Classic catalog: {name}"
            );
        }
        let unique_editor = EDITOR_SYNTAX_THEME_CHOICES
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        assert_eq!(unique_editor.len(), EDITOR_SYNTAX_THEME_CHOICES.len());
        for name in EDITOR_SYNTAX_THEME_CHOICES {
            assert!(
                classic_editor_catalog.contains(&format!("'{name}'")),
                "GPUI editor theme missing from Classic catalog: {name}"
            );
        }
    }

    #[test]
    fn modal_matches_classic_proportions_and_stays_within_viewport() {
        let (width, height, stacked) = modal_size(1440.0, 900.0);
        assert!((width - 1324.8).abs() < 0.01);
        assert!((height - 828.0).abs() < 0.01);
        assert!(!stacked);
        assert_eq!(modal_size(4000.0, 2400.0), (1800.0, 1280.0, false));
        assert!(modal_size(640.0, 480.0).2);
    }
}
