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
// Classic chip content inset is AleraTokens.space6.
pub const TAB_CHIP_HORIZONTAL_PADDING: f32 = 6.0;
pub const TAB_CLOSE_ICON_SIZE: f32 = 12.0;
pub const TAB_CLOSE_TARGET_SIZE: f32 = 16.0; // Classic: 12px icon + 2px padding on each side.
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

/// Visible primary Terminal dimensions after the *inner* Terminal split.
/// The PTY grid and Windows IME candidate anchor must use the same bounds:
/// passing the unsplit workbench size lets candidates drift into the other pane.
pub fn primary_inner_terminal_render_size(
    pane_width: f32,
    pane_height: f32,
    split_exists: bool,
    side_by_side: bool,
) -> (f32, f32) {
    if !pane_width.is_finite() || !pane_height.is_finite() {
        return (0.0, 0.0);
    }
    let width = pane_width / if split_exists && side_by_side { 2.0 } else { 1.0 };
    let height = pane_height / if split_exists && !side_by_side { 2.0 } else { 1.0 };
    (width.max(0.0), height.max(0.0))
}

/// Each legacy inner Terminal split renders a compact 28px focus header
/// above the secondary VT scroll area, even when panes are side-by-side.
pub const TERMINAL_INNER_SPLIT_HEADER_HEIGHT: f32 = 28.0;

/// Remaining terminal content height after the inner split header.
/// The Host PTY must not be given rows that fit behind that header.
pub fn inner_secondary_terminal_render_height(primary_height: f32, side_by_side: bool) -> f32 {
    if !primary_height.is_finite() {
        return 0.0;
    }
    (primary_height / if side_by_side { 1.0 } else { 2.0 }
        - TERMINAL_INNER_SPLIT_HEADER_HEIGHT)
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
    fn classic_agent_working_motion_is_limited_to_sidebar_and_compact_tray() {
        let renderer = include_str!("native_agent_quota_ui.rs");
        assert!(renderer.contains("fn agent_run_indicator("));
        assert!(renderer.contains("Spinner::new().with_size(px(size))"));
        assert!(renderer.contains("state.run_indicator_color()"));
        let main = include_str!("main.rs");
        let tab = main
            .split_once("fn render_mixed_tab_chip(")
            .unwrap()
            .1
            .split_once("fn sync_mixed_tabs(")
            .unwrap()
            .0;
        // Classic Tab strip uses the blue 6px attention dot, not an
        // independently ticking Spinner per open Tab.
        assert!(tab.contains("TAB_ATTENTION_DOT_SIZE"));
        assert!(!tab.contains("Spinner::new()"));
        assert!(tab.contains(".tooltip(agent_type, locale)"));
        assert!(tab.contains("tab_tooltip_title = agent_tooltip.clone()"));
        let sidebar = main
            .split_once(r#".id("terminal-sessions-scroll")"#)
            .unwrap()
            .1;
        assert!(sidebar.contains("agent_run_indicator("));
        assert!(sidebar.contains("if state == AgentVisualState::Working"));
        assert!(sidebar.contains("state.tooltip(&brand, agent_ui_locale)"));
    }

    #[test]
    fn agent_status_summary_and_host_row_share_live_only_state_colors() {
        let source = include_str!("main.rs");
        // Both summary surfaces must render from the same *current* Host
        // inventory, not from the last stale Terminal output or local profile.
        assert!(source.contains(
            "AgentStateSummary::from_sessions(&self.terminals.sessions, agent_inventory_current)"
        ));
        assert!(source.contains("AgentVisualState::for_session(session, agent_inventory_current)"));
        // Keep the rendered calls, but do not couple this test to rustfmt
        // indentation or whether the helper is namespace-qualified.
        let compact = source.split_whitespace().collect::<String>();
        assert!(compact.contains("agent_state_summary_chips(agent_state_summary,agent_ui_locale,false,"));
        assert!(compact.contains("agent_state_summary_chips(agent_state_summary,agent_ui_locale,true,"));
        assert!(source.contains("let inventory_current = !self.terminals.session_list_is_stale();"));
        assert!(source.contains("if is_agent && inventory_current && tab.running"));
        assert!(source.contains("AgentVisualState::from_hook(state, tab.interrupted).color()"));
        // Status never alters the separate 6px Tab attention slot or the
        // Agent profile selection marker.
        assert!(source.contains(".w(px(TAB_ATTENTION_DOT_SIZE))"));
        assert!(source.contains("if selected { \"●\" } else { \"○\" }"));
    }

    #[test]
    fn native_delete_and_restore_icons_follow_classic_lucide_registry() {
        let classic = include_str!("../../lib/src/design_system/icons/alera_icons.dart");
        assert!(classic.contains("static const IconData delete = LucideIcons.trash2;"));
        assert!(classic.contains("static const IconData restore = LucideIcons.history;"));
        let native = include_str!("native_ui_icons.rs");
        // GPUI Kit v0.9 calls the Classic trash2 artwork "Trash";
        // its packaged SVG has the same two interior bin strokes.
        assert!(native.contains("UiIcon::Delete => LucideIcon::Trash,"));
        // GPUI Kit lacks a History variant entirely, so embed the actual
        // Classic Lucide stroke paths rather than showing a plain clock.
        assert!(native.contains("UiIcon::History => return None"));
        assert!(native.contains("CLASSIC_HISTORY_SVG"));
        assert!(native.contains(".data(CLASSIC_HISTORY_SVG)"));
        assert!(native.contains("M12 7v5l4 2"));
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
    fn git_original_and_explorer_preview_tabs_share_classic_chip_padding() {
        let source = include_str!("main.rs");
        // Secondary Tab types must not retain the old nested 4px + 4px
        // padding while mixed Host/Editor chips use Classic's 6px inset.
        for (outer_id, inner_id) in [
            (r#".id(("git-original-tab", index))"#, r#".id(("git-original-tab-select", index))"#),
            (r#".id(("file-preview-tab", index))"#, r#".id(("file-tab", index))"#),
        ] {
            let tab = source.split_once(outer_id).unwrap().1;
            let outer = tab.split_once(".child(").unwrap().0;
            assert!(outer.contains(".px(px(TAB_CHIP_HORIZONTAL_PADDING))"), "{outer_id}: chip has wrong horizontal inset");
            let selection = tab.split_once(inner_id).unwrap().1
                .split_once(".cursor_pointer()").unwrap().0;
            assert!(!selection.contains(".px_1()"), "{inner_id}: old nested padding drifts from Classic");
        }
    }

    #[test]
    fn mixed_tab_chip_matches_classic_six_pixel_inset_and_compact_close_target() {
        let classic = include_str!("../../lib/src/features/workbench/presentation/workspace_workbench_tab_chips.dart");
        assert!(classic.contains("horizontal: AleraTokens.space6,"));
        assert!(classic.contains("padding: EdgeInsets.all(2),"));
        assert!(classic.contains("size: 12,"));
        assert_eq!(TAB_CHIP_HORIZONTAL_PADDING, 6.0);
        assert_eq!(TAB_CLOSE_ICON_SIZE, 12.0);
        assert_eq!(TAB_CLOSE_TARGET_SIZE, 16.0);
        let source = include_str!("main.rs");
        let chip = source.split_once("fn render_mixed_tab_chip(").unwrap().1
            .split_once("fn sync_mixed_tabs(").unwrap().0;
        let outer = chip.split_once(".id((chip_id, index))").unwrap().1
            .split_once(".child(").unwrap().0;
        assert!(outer.contains(".px(px(TAB_CHIP_HORIZONTAL_PADDING))"));
        let focus = chip.split_once(".id((focus_id, index))").unwrap().1
            .split_once(".cursor_pointer()").unwrap().0;
        assert!(!focus.contains(".px_1()"), "Nested chip padding makes the icon 8px from the edge instead of Classic 6px");
        assert!(chip.contains(".h(px(32.0))"));
        assert!(chip.contains(".mr_2()"));
    }

    #[test]
    fn secondary_tab_blank_space_creates_an_editor_in_both_split_directions() {
        let source = include_str!("main.rs");
        // Both side-by-side and stacked secondary panes need the Classic
        // double-click-to-create target, scoped to the secondary pane.
        assert_eq!(source.matches(r#".id("mixed-tabs-empty-area")"#).count(), 1);
        let marker = r#".id("mixed-secondary-tabs-empty-area")"#;
        assert_eq!(source.matches(marker).count(), 2);
        for secondary in source.split(marker).skip(1) {
            let blank_area = secondary.split(".on_drop(").next().unwrap();
            assert!(blank_area.contains("event.click_count() == 2"));
            assert!(blank_area.contains("this.new_editor_in_pane(PaneSlot::Secondary, cx);"));
        }
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
    fn inner_terminal_header_keeps_its_reserved_height_and_truncates_long_titles() {
        let source = include_str!("main.rs");
        let focus = source.split_once(r#".id("terminal-secondary-focus")"#).unwrap().1
            .split_once(".on_click(").unwrap().0;
        assert!(focus.contains(".h(px(TERMINAL_INNER_SPLIT_HEADER_HEIGHT))"));
        assert!(focus.contains(".flex_shrink_0()"), "focus header must not shrink below PTY-reserved 28px");
        assert!(focus.contains(".flex()") && focus.contains(".items_center()"), "focus text must be vertically centered");
        assert!(focus.contains(".min_w_0()"), "focus header must fit narrow split widths");
        assert!(focus.contains(".child(div().min_w_0().truncate().child(format!("), "long Terminal titles must truncate inside the split");
        let transcript = source.split_once(r#".id("terminal-output-scroll")"#).unwrap().1
            .split_once(".child(if terminal_output.is_empty()").unwrap().0;
        assert!(transcript.contains(".min_h_0()"), "scrollable transcript must shrink with split height");
        assert!(transcript.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
    }

    #[test]
    fn legacy_terminal_inner_split_has_directional_seam_and_full_bleed_background() {
        // The inner Terminal split is distinct from mixed Workbench panes.
        // Both views must inherit the same VT canvas even with long output.
        let source = include_str!("main.rs");
        let preview = source
            .split_once(r#".id("terminal-secondary-preview")"#)
            .unwrap()
            .1
            .split_once(".child(div()")
            .unwrap()
            .0;
        assert!(preview.contains(".min_w_0()"));
        assert!(preview.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
        assert!(preview.contains(".when(terminal_split_horizontal, |pane| pane.border_l_1())"));
        assert!(preview.contains(".when(!terminal_split_horizontal, |pane| pane.border_t_1())"));
        let secondary_scroll = source
            .split_once(r#".id("terminal-secondary-scroll")"#)
            .unwrap()
            .1
            .split_once(".child(output)")
            .unwrap()
            .0;
        assert!(secondary_scroll.contains(".min_w_0()"));
        assert!(secondary_scroll.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
        assert!(secondary_scroll.contains(".px_3()"));
        assert!(secondary_scroll.contains(".py_3()"));
    }

    #[test]
    fn split_pane_headers_and_terminal_seams_match_split_direction() {
        let source = include_str!("main.rs");
        for id in ["mixed-workbench-tab-strip", "mixed-secondary-toolbar"] {
            let needle = format!(r#".id("{id}")"#);
            let mut found = 0;
            for row in source.split(&needle).skip(1) {
                let style = row.split(".child(").next().unwrap_or(row);
                assert!(style.contains(".h(px(TAB_STRIP_HEIGHT))"), "{id}: row height drift");
                assert!(style.contains(".items_center()"), "{id}: row alignment drift");
                found += 1;
            }
            assert!(found > 0);
        }
        let secondary = source.split_once(r#".id("mixed-secondary-pane")"#).unwrap().1
            .split_once(".child(if mixed_split_horizontal {").unwrap().0;
        assert!(secondary.contains(".when(mixed_split_horizontal, |pane| pane.border_l_1())"));
        assert!(secondary.contains(".when(!mixed_split_horizontal, |pane| pane.border_t_1())"));
        let vt = source.split_once(r#".id("mixed-secondary-content")"#).unwrap().1
            .split_once(".key_context(").unwrap().0;
        assert!(vt.contains(".when(mixed_split_horizontal, |panel| panel.border_t_1())"));
        assert!(vt.contains(".bg(rgb(TERMINAL_DEFAULT_BG))"));
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
    fn inner_terminal_pty_and_ime_share_visible_split_bounds() {
        assert_eq!(primary_inner_terminal_render_size(1000.0, 600.0, false, true), (1000.0, 600.0));
        assert_eq!(primary_inner_terminal_render_size(1000.0, 600.0, false, false), (1000.0, 600.0));
        assert_eq!(primary_inner_terminal_render_size(1000.0, 600.0, true, true), (500.0, 600.0));
        assert_eq!(primary_inner_terminal_render_size(1000.0, 600.0, true, false), (1000.0, 300.0));
        assert_eq!(primary_inner_terminal_render_size(-20.0, 600.0, true, true), (0.0, 600.0));
        assert_eq!(primary_inner_terminal_render_size(f32::NAN, 600.0, true, true), (0.0, 0.0));
        assert_eq!(primary_inner_terminal_render_size(1000.0, f32::INFINITY, true, false), (0.0, 0.0));
        // The same dimensions must bound the actual PTY cell count and IME
        // popup position, not just the floating-point layout calculation.
        for side_by_side in [true, false] {
            let (width, height) = primary_inner_terminal_render_size(
                1000.0, 600.0, true, side_by_side,
            );
            let (cols, rows) = crate::terminal::estimate_terminal_viewport_with_line_height(
                width, height, 14.0, 1.3,
            ).expect("inner split should fit Terminal cells");
            let (ime_x, ime_y) = crate::terminal::terminal_ime_anchor_with_line_height(
                (100, 120), width, height, 14.0, 1.3,
            ).expect("inner split must fit IME cursor");
            assert!(ime_x <= width - 2.0 && ime_y <= height - 2.0);
            if side_by_side {
                assert!(cols < 70, "half-width Terminal must have fewer columns");
            } else {
                assert!(rows < 20, "half-height Terminal must have fewer rows");
            }
        }
        let source = include_str!("main.rs");
        assert!(source.contains("let (primary_terminal_width, primary_terminal_height) = primary_inner_terminal_render_size("));
        let anchor = source.split_once("let primary_ime_anchor =").unwrap().1
            .split_once("let secondary_ime_anchor =").unwrap().0;
        assert!(anchor.contains("primary_terminal_width,"), "primary IME must not use unsplit width");
        assert!(anchor.contains("primary_terminal_height,"), "primary IME must not use unsplit height");
        let resize = source.split_once("let mut proposed_viewports =").unwrap().1
            .split_once("if terminal_split_exists {").unwrap().0;
        assert!(resize.contains("let width = primary_terminal_width;"));
        assert!(resize.contains("let height = primary_terminal_height;"));
    }

    #[test]
    fn inner_terminal_split_viewport_reserves_focus_header_in_both_orientations() {
        assert_eq!(TERMINAL_INNER_SPLIT_HEADER_HEIGHT, 28.0);
        assert_eq!(inner_secondary_terminal_render_height(500.0, true), 472.0);
        assert_eq!(inner_secondary_terminal_render_height(500.0, false), 222.0);
        assert_eq!(inner_secondary_terminal_render_height(12.0, true), 0.0);
        assert_eq!(inner_secondary_terminal_render_height(12.0, false), 0.0);
        assert_eq!(inner_secondary_terminal_render_height(f32::NAN, true), 0.0);
        assert_eq!(inner_secondary_terminal_render_height(f32::INFINITY, false), 0.0);
        let source = include_str!("main.rs");
        assert!(source.contains(".h(px(TERMINAL_INNER_SPLIT_HEADER_HEIGHT))"));
        assert!(source.contains("let height = inner_secondary_terminal_render_height("));
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
