//! Identity and on-disk state contracts for the independent Alera Native app.
//! The shipped Flutter executable keeps its existing Alera identity and
//! update channel; the two applications can run simultaneously.
use std::path::{Path, PathBuf};

pub const NATIVE_APP_NAME: &str = "Alera Native";
pub const NATIVE_WINDOWS_APP_ID: &str = "dev.leynier.alera.native";
pub const CLASSIC_WINDOWS_APP_ID: &str = "dev.leynier.alera";
pub const CLASSIC_DEV_WINDOWS_APP_ID: &str = "dev.leynier.alera.dev";
pub const NATIVE_EXE_NAME: &str = "AleraNative.exe";
pub const NATIVE_PREVIEW_CHANNEL: &str = "native-preview";

/// Fresh installs store all Native-only tab order, scratch recovery, projects
/// and pane layouts outside of the Classic app's application-data subtree.
pub fn native_state_directory(base: &Path) -> PathBuf {
    base.join("AleraNative")
}

/// Existing experimental GPUI users may have unreleased scratch drafts in
/// `Alera/gpui-preview`. Keep that directory accessible until they choose
/// to migrate, rather than silently ignoring or moving unsaved documents.
pub fn native_state_directory_preserving_preview(base: &Path) -> PathBuf {
    let preferred = native_state_directory(base);
    let previous = base.join("Alera").join("gpui-preview");
    if !preferred.exists() && previous.is_dir() {
        previous
    } else {
        preferred
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_classic_have_distinct_windows_identity_and_distribution() {
        assert_eq!(NATIVE_APP_NAME, "Alera Native");
        assert_ne!(NATIVE_WINDOWS_APP_ID, CLASSIC_WINDOWS_APP_ID);
        assert_ne!(NATIVE_WINDOWS_APP_ID, CLASSIC_DEV_WINDOWS_APP_ID);
        assert_ne!(NATIVE_EXE_NAME.to_ascii_lowercase(), "alera.exe");
        assert_ne!(NATIVE_PREVIEW_CHANNEL, "stable");
        assert_ne!(NATIVE_PREVIEW_CHANNEL, "dev");
    }

    #[test]
    fn new_native_state_is_separate_but_existing_preview_drafts_remain_accessible() {
        let fixture = tempfile::tempdir().unwrap();
        let base = fixture.path();
        let expected = base.join("AleraNative");
        assert_eq!(native_state_directory_preserving_preview(base), expected);
        let previous = base.join("Alera").join("gpui-preview");
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::write(previous.join("scratch-draft.json"), "keep recovery").unwrap();
        assert_eq!(native_state_directory_preserving_preview(base), previous);
        assert_eq!(
            std::fs::read_to_string(previous.join("scratch-draft.json")).unwrap(),
            "keep recovery"
        );
        std::fs::create_dir_all(&expected).unwrap();
        assert_eq!(native_state_directory_preserving_preview(base), expected);
        assert_eq!(
            std::fs::read_to_string(previous.join("scratch-draft.json")).unwrap(),
            "keep recovery"
        );
    }
}
