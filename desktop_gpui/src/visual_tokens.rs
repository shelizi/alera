//! Flutter desktop shell tokens used by the experimental GPUI renderer.
//! The Flutter source of truth is lib/src/app/theme/alera_tokens.dart.
//! This is visual parity only: do not conflate it with functional parity.

pub const BG: u32 = 0x101010;
pub const PANEL: u32 = 0x202020; // Flutter surfaceVariant.
pub const TOOLBAR: u32 = 0x181818; // Flutter surface.
pub const ELEVATED: u32 = 0x242424;
pub const BORDER: u32 = 0x272727; // Flutter borderSubtle.
pub const BORDER_ACTIVE: u32 = 0x323232; // Flutter border (active tab outline).
pub const TEXT: u32 = 0xf5f5f5;
pub const MUTED: u32 = 0xa1a1a1;
pub const FAINT: u32 = 0x606060;
pub const ACTIVE: u32 = 0x303030;
// Classic AgentStatusDot uses AleraTokens.info/warning/error/success.
pub const AGENT_WORKING: u32 = 0x60A5FA;
pub const AGENT_WAITING: u32 = 0xF59E0B;
pub const AGENT_BLOCKED: u32 = 0xF87171;
pub const AGENT_DONE: u32 = 0x22C55E;

pub const TOP_BAR_HEIGHT: f32 = 52.0;
pub const SECTION_HEADER_HEIGHT: f32 = 44.0;
pub const STATUS_BAR_HEIGHT: f32 = 30.0;
pub const PROJECTS_SIDEBAR_WIDTH: f32 = 300.0;
pub const PROJECTS_COLLAPSED_WIDTH: f32 = 52.0; // Classic sidebarCollapsedWidth.
pub const RIGHT_SIDEBAR_WIDTH: f32 = 315.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 460.0;
pub const RIGHT_SIDEBAR_MAX_WIDTH: f32 = 1600.0;
pub const MIN_EDITOR_WIDTH: f32 = 360.0;
pub const RESIZE_CHROME: f32 = 14.0;
pub const SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 6.0;
pub const TAB_STRIP_HEIGHT: f32 = 44.0;
// Classic's tab rows use space16 from the outer pane edge.
pub const TAB_STRIP_LEADING_PADDING: f32 = 16.0;
// Classic keeps an attention-dot slot even without an agent status.
pub const TAB_ATTENTION_DOT_SIZE: f32 = 6.0;
pub const TAB_CLOSE_ICON_SIZE: f32 = 12.0;
pub const TAB_CLOSE_TARGET_SIZE: f32 = 18.0;
pub const EDITOR_TOOLBAR_HEIGHT: f32 = 29.0;

/// A New Tab menu contains Agent discovery and optional Host sections, unlike
/// Classic's short two-action picker. Let users see more of the list on large
/// windows, but never make a short viewport scroll its entire menu off-screen.
pub fn new_tab_menu_max_height(viewport_height: f32) -> f32 {
    if !viewport_height.is_finite() || viewport_height <= 0.0 {
        return 320.0;
    }
    (viewport_height - 96.0).max(0.0).min(560.0)
}
// The native Terminal viewport uses a one-pixel top border and pane seams.
// Keep a two-pixel safety margin instead of subtracting an unrelated 14px.
const TERMINAL_VIEWPORT_BORDER_SLACK: f32 = 2.0;

/// GPUI's viewport is its client rectangle, not the external Windows title
/// bar. Reserve exactly the shell/header heights actually rendered so the
/// Host PTY grows to the full available Terminal area.
pub fn usable_workbench_height(
    window_height: f32,
    primary_pinned_row_visible: bool,
    editor_toolbar_visible: bool,
) -> f32 {
    if !window_height.is_finite() {
        return 0.0;
    }
    (window_height
        - TOP_BAR_HEIGHT
        - STATUS_BAR_HEIGHT
        - TAB_STRIP_HEIGHT * (1.0 + f32::from(primary_pinned_row_visible))
        - if editor_toolbar_visible {
            EDITOR_TOOLBAR_HEIGHT
        } else {
            0.0
        }
        - TERMINAL_VIEWPORT_BORDER_SLACK)
        .max(0.0)
}

/// In a side-by-side split both Tab strips sit above the panes. In a
/// stacked split the secondary pane owns an additional 44px (or 88px)
/// header inside its half of the content region.
pub fn secondary_terminal_render_height(
    allocated_pane_height: f32,
    stacked_split: bool,
    secondary_pinned_row: bool,
) -> f32 {
    if !allocated_pane_height.is_finite() {
        return 0.0;
    }
    (allocated_pane_height
        - if stacked_split {
            TAB_STRIP_HEIGHT * (1.0 + f32::from(secondary_pinned_row))
        } else {
            0.0
        })
    .max(0.0)
}

/// A drag can never make the center unusable or exceed Classic's sidebar
/// preferences. On very small windows, the sidebar keeps its 220px minimum.
pub fn clamp_projects_sidebar_width(requested: f32, viewport: f32, context: f32) -> f32 {
    let max = (viewport - context - MIN_EDITOR_WIDTH - RESIZE_CHROME)
        .max(SIDEBAR_MIN_WIDTH)
        .min(SIDEBAR_MAX_WIDTH);
    requested.clamp(SIDEBAR_MIN_WIDTH, max)
}

pub fn clamp_context_sidebar_width(requested: f32, viewport: f32, projects: f32) -> f32 {
    let max = (viewport - projects - MIN_EDITOR_WIDTH - RESIZE_CHROME)
        .max(SIDEBAR_MIN_WIDTH)
        .min(RIGHT_SIDEBAR_MAX_WIDTH);
    requested.clamp(SIDEBAR_MIN_WIDTH, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guard against new Flutter theme changes silently drifting away from
    // the GPUI MVP. The dimensions are from the same AleraTokens definition.
    const FLUTTER: &str = include_str!("../../lib/src/app/theme/alera_tokens.dart");

    #[test]
    fn shell_colors_follow_flutter_source_of_truth() {
        for (name, value) in [
            ("bg", BG),
            ("surface", TOOLBAR),
            ("surfaceVariant", PANEL),
            ("surfaceElevated", ELEVATED),
            ("borderSubtle", BORDER),
            ("border", BORDER_ACTIVE),
            ("foreground", TEXT),
            ("foregroundMuted", MUTED),
            ("foregroundFaint", FAINT),
        ] {
            let token = format!("static const Color {name} = Color(0xFF{value:06X});");
            assert!(FLUTTER.contains(&token), "Flutter token drift: {name}");
        }
    }

    #[test]
    fn shell_geometry_follows_flutter_source_of_truth() {
        for (name, value) in [
            ("topBarHeight", TOP_BAR_HEIGHT),
            ("sidebarHeaderHeight", SECTION_HEADER_HEIGHT),
            ("statusBarHeight", STATUS_BAR_HEIGHT),
            ("sidebarDefaultWidth", PROJECTS_SIDEBAR_WIDTH),
            ("sidebarCollapsedWidth", PROJECTS_COLLAPSED_WIDTH),
            ("sidebarMinWidth", SIDEBAR_MIN_WIDTH),
            ("sidebarMaxWidth", SIDEBAR_MAX_WIDTH),
            ("rightSidebarMaxWidth", RIGHT_SIDEBAR_MAX_WIDTH),
            ("rightSidebarMinEditorWidth", MIN_EDITOR_WIDTH),
        ] {
            let token = format!("static const double {name} = {value:.1};");
            assert!(FLUTTER.contains(&token), "Flutter dimension drift: {name}");
        }
        assert!(TAB_STRIP_HEIGHT >= SECTION_HEADER_HEIGHT);
        assert!(RIGHT_SIDEBAR_WIDTH >= 220.0);
    }

    #[test]
    fn classic_tab_chip_attention_dot_precedes_title_and_keeps_its_slot() {
        let classic = include_str!(
            "../../lib/src/features/workbench/presentation/workspace_workbench_tab_chips.dart"
        );
        assert!(classic.contains("dimension: 6"));
        let source = include_str!("main.rs");
        let chip = source
            .split_once("fn render_mixed_tab_chip(")
            .unwrap()
            .1
            .split_once("/// Project both original state owners")
            .unwrap()
            .0;
        let dot = chip.find(".w(px(TAB_ATTENTION_DOT_SIZE))").unwrap();
        let title = chip.find(r#".id(("mixed-tab-title", index))"#).unwrap();
        assert!(
            dot < title,
            "Agent status dot must stay before the tab title"
        );
        assert!(chip.contains("div().w(px(TAB_ATTENTION_DOT_SIZE))"));
    }

    #[test]
    fn agent_brand_icons_reach_all_split_tabs_and_sidebar_rows() {
        let source = include_str!("main.rs");
        let chip = source
            .split_once("fn render_mixed_tab_chip(")
            .unwrap()
            .1
            .split_once("fn sync_mixed_tabs(")
            .unwrap()
            .0;
        assert!(chip.contains("tab.verified_agent_brand(host_sessions)"));
        assert!(chip.contains("native_agent_quota_ui::quota_provider_icon("));
        assert!(chip.contains("TAB_ATTENTION_DOT_SIZE"));
        let tab_calls = source
            .split("Self::render_mixed_tab_chip(")
            .skip(1)
            .collect::<Vec<_>>();
        assert_eq!(tab_calls.len(), 6);
        for call in tab_calls {
            assert!(
                call.split_once(')').unwrap().0.contains("&self.terminals.sessions"),
                "All pinned/unpinned Primary/Secondary/stacked Tab paths must use current Host branding",
            );
        }
        let sidebar = source
            .split_once(r#".id("agent-profiles-list")"#)
            .unwrap()
            .1;
        assert!(sidebar.contains("quota_provider_icon(&brand, 16.0)"));
        assert!(sidebar.contains("quota_provider_icon(&brand, 14.0)"));
    }

    #[test]
    fn classic_tab_rows_keep_sixteen_pixel_leading_gutter_and_compact_close() {
        let classic = include_str!(
            "../../lib/src/features/workbench/presentation/workspace_workbench_tab_strip.dart"
        );
        assert!(classic.contains("const SizedBox(width: AleraTokens.space16)"));
        let source = include_str!("main.rs");
        for id in [
            "mixed-primary-pinned-tab-strip",
            "mixed-tabs-scroll",
            "mixed-secondary-tabs-scroll",
        ] {
            let start = source.find(&format!(r#".id("{id}")"#)).unwrap();
            let style = &source[start..start + 550];
            assert!(
                style.contains(".pl(px(TAB_STRIP_LEADING_PADDING))"),
                "Tab row {id} lacks Classic's leading inset"
            );
        }
        assert!(source.matches("tab_close_button(").count() >= 3);
    }

    #[test]
    fn new_tab_popup_fits_the_client_height_and_shows_more_agents() {
        assert_eq!(new_tab_menu_max_height(1040.0), 560.0);
        assert_eq!(new_tab_menu_max_height(400.0), 304.0);
        assert_eq!(new_tab_menu_max_height(200.0), 104.0);
        assert_eq!(new_tab_menu_max_height(f32::NAN), 320.0);
        let source = include_str!("native_popup_ui.rs");
        let menu = source
            .split_once("pub(super) fn kit_new_tab_menu(")
            .unwrap()
            .1
            .split_once("pub(super) fn kit_pane_actions_menu(")
            .unwrap()
            .0;
        assert!(menu.contains("new_tab_menu_max_height("));
        // Formatting must not affect this structural UX guard.
        let compact = menu.split_whitespace().collect::<String>();
        assert!(compact.contains(".separator().label(tr(locale,\"AttachrunningHostsession\"))"));
    }

    #[test]
    fn editor_find_bar_does_not_consume_a_row_while_closed() {
        let find_ui = include_str!("editor_find_ui.rs");
        let closed = find_ui
            .split_once("if !self.find.visible {")
            .unwrap()
            .1
            .split_once("let search = self.find.search(cx);")
            .unwrap()
            .0;
        assert!(!closed.contains("Find / Replace"));
        assert!(closed.contains(".h(px(0.0))"));
        let editor = include_str!("editor_ui.rs");
        assert!(editor.contains(r#""editor-show-find""#));
        assert!(editor.contains("this.show_find(&ShowFind, window, cx)"));
    }

    #[test]
    fn editor_status_is_single_line_but_plaintext_warning_remains_visible() {
        let editor = include_str!("editor_ui.rs");
        let status = editor
            .split_once(r#".id("editor-compact-status")"#)
            .unwrap()
            .1
            .split_once(".child(")
            .unwrap()
            .0;
        assert!(status.contains(".whitespace_nowrap()"));
        assert!(status.contains(".truncate()"));
        assert!(editor.contains("Drafts: plaintext"));
        assert!(editor.contains("RECOVERY_PLAINTEXT_NOTICE"));
    }

    #[test]
    fn sidebar_resize_preserves_editor_space_and_bounds() {
        assert_eq!(clamp_projects_sidebar_width(100.0, 1600.0, 315.0), 220.0);
        assert_eq!(clamp_projects_sidebar_width(900.0, 1600.0, 315.0), 460.0);
        assert_eq!(clamp_context_sidebar_width(900.0, 1000.0, 300.0), 326.0);
        assert_eq!(clamp_context_sidebar_width(900.0, 2400.0, 300.0), 900.0);
        assert_eq!(clamp_context_sidebar_width(5000.0, 3500.0, 300.0), 1600.0);
    }

    #[test]
    fn collapsed_panels_stay_hidden_after_flex_display_is_applied() {
        // GPUI Styled::flex() sets display:Flex and silently overrides a
        // previous hidden() (display:None). A GUI smoke test caught both
        // expanded and collapsed project panels on screen together.
        let source = include_str!("main.rs");
        for id in [
            "native-right-sidebar-scroll",
            "right-explorer-header",
            "right-search-header",
            "right-references-panel",
            "right-pr-browser-panel",
            "right-sidebar-rail",
            "projects-sidebar-expanded",
            "projects-sidebar-rail",
        ] {
            let needle = format!(r#".id("{id}")"#);
            let start = source
                .find(&needle)
                .unwrap_or_else(|| panic!("Missing panel {id}"));
            let region = &source[start + needle.len()..];
            let style = region.split(".child(").next().unwrap_or(region);
            let hidden = style
                .find("|")
                .and_then(|_| style.find(".hidden()"))
                .unwrap_or_else(|| panic!("{id} missing hidden() guard"));
            let flex = style
                .find(".flex()")
                .unwrap_or_else(|| panic!("{id} missing display:Flex"));
            assert!(
                flex < hidden,
                "{id}: .flex() overrides .hidden() when called after it"
            );
        }
    }

    #[test]
    fn sidebars_keep_width_when_center_contains_long_content() {
        // Without shrink=0, a long terminal line or tab label can squeeze
        // the right sidebar down to a sliver and place the remainder offscreen.
        let source = include_str!("main.rs");
        for id in [
            "native-right-sidebar-scroll",
            "right-sidebar-rail",
            "right-sidebar-resize-handle",
            "projects-sidebar-expanded",
            "projects-sidebar-rail",
            "projects-sidebar-resize-handle",
        ] {
            let needle = format!(r#".id("{id}")"#);
            let start = source
                .find(&needle)
                .unwrap_or_else(|| panic!("Missing panel {id}"));
            let region = &source[start + needle.len()..];
            let style = region.split(".child(").next().unwrap_or(region);
            assert!(
                style.contains(".flex_shrink_0()"),
                "{id} must not shrink in the shell row"
            );
        }
    }

    #[test]
    fn terminal_full_width_uses_one_background_for_viewport_and_gutters() {
        // The shell color (#101010) differs from VT (#15171B). Unused
        // strips, splits and padding must inherit VT, not the shell.
        let source = include_str!("main.rs");
        let pane = source
            .split_once(r#".id("mixed-primary-pane")"#)
            .unwrap()
            .1
            .split_once(".child(")
            .unwrap()
            .0;
        assert!(pane.contains(".min_w_0()"));
        assert!(pane.contains("pane.bg(rgb(TERMINAL_DEFAULT_BG))"));
        let outer = source
            .split_once(".child(if has_terminal_preview {")
            .unwrap()
            .1
            .split_once(".child(if terminal_is_vt {")
            .unwrap()
            .0;
        assert!(outer.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
        assert!(!outer.contains(".bg(rgb(BG))"));
        let viewport = source
            .split_once(r#".id("terminal-colored-grid")"#)
            .unwrap()
            .1
            .split_once(".children(terminal_screen_rows")
            .unwrap()
            .0;
        assert!(viewport.contains(".flex_1()"));
        assert!(viewport.contains(".min_w_0()"));
        assert!(viewport.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
        let secondary = source
            .split_once(r#".id("mixed-secondary-content")"#)
            .unwrap()
            .1
            .split_once(".child(if let Some(rows)")
            .unwrap()
            .0;
        assert!(secondary.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
    }

    #[test]
    fn terminal_viewport_fits_real_shell_chrome_and_pinned_tabs() {
        assert_eq!(usable_workbench_height(800.0, false, false), 672.0);
        assert_eq!(usable_workbench_height(800.0, true, false), 628.0);
        assert_eq!(usable_workbench_height(800.0, false, true), 643.0);
        assert_eq!(usable_workbench_height(800.0, true, true), 599.0);
        assert_eq!(usable_workbench_height(f32::NAN, false, false), 0.0);
        assert_eq!(usable_workbench_height(20.0, false, false), 0.0);
    }

    #[test]
    fn secondary_terminal_height_subtracts_only_its_actual_stacked_header() {
        assert_eq!(secondary_terminal_render_height(300.0, false, false), 300.0);
        assert_eq!(secondary_terminal_render_height(300.0, false, true), 300.0);
        assert_eq!(secondary_terminal_render_height(300.0, true, false), 256.0);
        assert_eq!(secondary_terminal_render_height(300.0, true, true), 212.0);
        assert_eq!(secondary_terminal_render_height(50.0, true, true), 0.0);
        assert_eq!(
            secondary_terminal_render_height(f32::INFINITY, true, false),
            0.0
        );
    }

    #[test]
    fn hook_state_colors_match_classic_status_dots() {
        for (name, color) in [
            ("info", AGENT_WORKING),
            ("warning", AGENT_WAITING),
            ("error", AGENT_BLOCKED),
            ("success", AGENT_DONE),
        ] {
            assert!(
                FLUTTER.contains(&format!(
                    "static const Color {name} = Color(0xFF{color:06X});"
                )),
                "GPUI agent Hook color drift: {name}",
            );
        }
    }
}
