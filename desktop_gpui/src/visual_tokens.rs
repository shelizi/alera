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
    fn sidebar_resize_preserves_editor_space_and_bounds() {
        assert_eq!(clamp_projects_sidebar_width(100.0, 1600.0, 315.0), 220.0);
        assert_eq!(clamp_projects_sidebar_width(900.0, 1600.0, 315.0), 460.0);
        assert_eq!(clamp_context_sidebar_width(900.0, 1000.0, 300.0), 326.0);
        assert_eq!(clamp_context_sidebar_width(900.0, 2400.0, 300.0), 900.0);
        assert_eq!(clamp_context_sidebar_width(5000.0, 3500.0, 300.0), 1600.0);
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
