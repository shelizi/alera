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

pub const TOP_BAR_HEIGHT: f32 = 52.0;
pub const SECTION_HEADER_HEIGHT: f32 = 44.0;
pub const STATUS_BAR_HEIGHT: f32 = 30.0;
pub const PROJECTS_SIDEBAR_WIDTH: f32 = 300.0;
pub const RIGHT_SIDEBAR_WIDTH: f32 = 315.0;
pub const TAB_STRIP_HEIGHT: f32 = 44.0;

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
        ] {
            let token = format!("static const double {name} = {value:.1};");
            assert!(FLUTTER.contains(&token), "Flutter dimension drift: {name}");
        }
        assert!(TAB_STRIP_HEIGHT >= SECTION_HEADER_HEIGHT);
        assert!(RIGHT_SIDEBAR_WIDTH >= 220.0);
    }
}
