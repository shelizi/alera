//! Shared sizing and dismissal policy for Native GPUI dialogs.
//!
//! Classic uses AleraDialog with a 92% Settings window, bounded to
//! 1800x1280. Keep geometry separate from rendering so future confirmation,
//! picker, and Settings modals reuse the same viewport-safe shell.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeModalSize {
    /// Settings: the large, split navigation/content dialog from Classic.
    Settings,
    /// A future confirmation/picker uses the same backdrop and frame.
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeModalDimensions {
    pub width: f32,
    pub height: f32,
    /// On a narrow window, navigation stacks above content instead of
    /// reducing the content pane to an unusable sliver.
    pub stacked_navigation: bool,
}

impl NativeModalSize {
    pub fn resolve(self, viewport_width: f32, viewport_height: f32) -> NativeModalDimensions {
        let width = viewport_width.max(0.0);
        let height = viewport_height.max(0.0);
        let (fraction, maximum_width, maximum_height) = match self {
            Self::Settings => (0.92, 1800.0, 1280.0),
            Self::Compact => (0.88, 560.0, 480.0),
        };
        let available_width = (width - 16.0).max(0.0);
        let available_height = (height - 16.0).max(0.0);
        let dialog_width = (width * fraction).min(maximum_width).min(available_width);
        let dialog_height = (height * fraction)
            .min(maximum_height)
            .min(available_height);
        NativeModalDimensions {
            width: dialog_width,
            height: dialog_height,
            stacked_navigation: self == Self::Settings && dialog_width < 720.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_modal_follows_classic_proportions_on_desktop() {
        let size = NativeModalSize::Settings.resolve(1440.0, 900.0);
        assert!((size.width - 1324.8).abs() < 0.01);
        assert!((size.height - 828.0).abs() < 0.01);
        assert!(!size.stacked_navigation);
        let large = NativeModalSize::Settings.resolve(4000.0, 2400.0);
        assert_eq!(large.width, 1800.0);
        assert_eq!(large.height, 1280.0);
    }

    #[test]
    fn settings_modal_stays_within_small_windows_and_stacks_navigation() {
        for (width, height) in [(680.0, 480.0), (320.0, 260.0), (16.0, 16.0)] {
            let size = NativeModalSize::Settings.resolve(width, height);
            assert!(size.width <= width - 16.0 + f32::EPSILON);
            assert!(size.height <= height - 16.0 + f32::EPSILON);
            assert!(size.stacked_navigation);
        }
    }

    #[test]
    fn compact_modals_share_geometry_without_settings_sidebar() {
        let size = NativeModalSize::Compact.resolve(1920.0, 1080.0);
        assert_eq!(size.width, 560.0);
        assert_eq!(size.height, 480.0);
        assert!(!size.stacked_navigation);
    }
}
