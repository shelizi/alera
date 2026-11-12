//! Versioned JSON persistence for the Editor-only subset of the GPUI
//! two-pane workbench layout.
//!
//! The runtime pane projection lives in `workbench_panes`; this module owns
//! only the on-disk V1 snapshot of *editor* pane membership: which editor
//! keys sit in the primary strip and the optional secondary strip, each
//! pane's active editor, the focused pane and the split axis. Editor keys
//! are opaque caller-defined strings — typically a stable encoding of an
//! editor tab instance — and this store treats them as inert data: it
//! never resolves them to paths, opens files, or interprets them.
//!
//! Host Terminal and Agent handles are deliberately unrepresentable: the
//! schema has no fields for them, so a saved layout can never resurrect,
//! attach or duplicate a Runtime Host PTY. Callers must pass only editor
//! keys; anything else simply will not round-trip.
//!
//! Records are normalized on both read and write: empty keys are dropped,
//! duplicates keep their first occurrence (the primary pane wins a key
//! listed in both panes), and active-editor or secondary-focus references
//! that no longer point into a pane's editor list are cleared. Unknown
//! `version` values and malformed JSON are rejected; a missing file loads
//! as `None`.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The only layout schema version this store reads or writes.
pub const WORKBENCH_LAYOUT_VERSION: u32 = 1;

const MAX_LAYOUT_BYTES: u64 = 64 * 1024;

/// Reserved opaque Editor key for a locally recovered, unsaved scratch draft.
/// No path or raw buffer contents enter the pane-layout JSON.
pub const SCRATCH_EDITOR_KEY_PREFIX: &str = "@alera-scratch:";

pub fn scratch_layout_key(id: &str) -> Option<String> {
    if id.len() != 56 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{SCRATCH_EDITOR_KEY_PREFIX}{id}"))
}

pub fn scratch_identity_from_layout_key(key: &str) -> Option<&str> {
    key.strip_prefix(SCRATCH_EDITOR_KEY_PREFIX)
        .filter(|id| id.len() == 56 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Split arrangement axis recorded in [`SavedWorkbenchLayoutV1`]. This is a
/// layout preference only; it never changes tab ownership.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedWorkbenchAxis {
    #[default]
    Horizontal,
    Vertical,
}

/// Pane holding keyboard focus in [`SavedWorkbenchLayoutV1`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedWorkbenchPane {
    #[default]
    Primary,
    Secondary,
}

/// Versioned on-disk snapshot of the Editor-only two-pane workbench layout.
///
/// Every editor reference is an opaque caller-supplied key; see the module
/// documentation for the normalization and Host-exclusion guarantees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWorkbenchLayoutV1 {
    /// Schema version; must equal [`WORKBENCH_LAYOUT_VERSION`]. Other
    /// versions are rejected on both read and write.
    pub version: u32,
    /// Split-enabled flag, recorded verbatim. The store does not derive it
    /// from pane contents.
    pub split_enabled: bool,
    /// Split arrangement axis preference.
    pub axis: SavedWorkbenchAxis,
    /// Pane holding keyboard focus.
    pub focused_pane: SavedWorkbenchPane,
    /// Editor keys in the primary strip, in strip order.
    pub primary_editors: Vec<String>,
    /// Editor keys in the secondary strip, in strip order.
    pub secondary_editors: Vec<String>,
    /// Active editor key of the primary pane, if any.
    pub primary_active_editor: Option<String>,
    /// Active editor key of the secondary pane, if any.
    pub secondary_active_editor: Option<String>,
    /// Optional file-backed Editor pin keys; Host PTY ids stay excluded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_editors: Vec<String>,
}

impl SavedWorkbenchLayoutV1 {
    /// Parse a V1 layout document. Malformed JSON, missing fields, invalid
    /// enum values and unknown versions are rejected; the returned record
    /// is normalized.
    pub fn from_json(text: &str) -> io::Result<Self> {
        let saved: Self = serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if saved.version != WORKBENCH_LAYOUT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported workbench layout version",
            ));
        }
        Ok(saved.normalized())
    }

    /// Serialize the normalized layout. Records stamped with an unknown
    /// version are refused so every file this store writes stays readable
    /// by [`Self::from_json`].
    pub fn to_json(&self) -> io::Result<String> {
        if self.version != WORKBENCH_LAYOUT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsupported workbench layout version",
            ));
        }
        serde_json::to_string(&self.normalized())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Load a V1 layout from `path`. A missing file yields `None`; corrupt
    /// or unknown-version content is an error, never silently ignored.
    pub fn load_from_path(path: &Path) -> io::Result<Option<Self>> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if metadata.len() > MAX_LAYOUT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Workbench layout state file exceeds 64 KiB",
            ));
        }
        let text = fs::read_to_string(path)?;
        Self::from_json(&text).map(Some)
    }

    /// Persist the normalized layout atomically: a same-directory temporary
    /// file is fully written and synced before it replaces `path`, so a
    /// crash mid-write keeps the previous complete snapshot.
    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        let json = self.to_json()?;
        write_bytes_atomic(path, json.as_bytes())
    }

    /// Canonical form of this record: deduplicated non-empty editor keys,
    /// pane-exclusive secondary keys, and active/focus references that still
    /// resolve inside their pane's editor list.
    fn normalized(&self) -> Self {
        let primary_editors = normalize_keys(&self.primary_editors);
        let mut secondary_editors = normalize_keys(&self.secondary_editors);
        secondary_editors.retain(|key| !primary_editors.contains(key));
        let mut pinned_editors = normalize_keys(&self.pinned_editors);
        pinned_editors
            .retain(|key| primary_editors.contains(key) || secondary_editors.contains(key));
        let primary_active_editor = self
            .primary_active_editor
            .clone()
            .filter(|key| primary_editors.contains(key));
        let secondary_active_editor = self
            .secondary_active_editor
            .clone()
            .filter(|key| secondary_editors.contains(key));
        let focused_pane = match self.focused_pane {
            SavedWorkbenchPane::Secondary if secondary_editors.is_empty() => {
                SavedWorkbenchPane::Primary
            }
            pane => pane,
        };
        Self {
            version: self.version,
            split_enabled: self.split_enabled,
            axis: self.axis,
            focused_pane,
            primary_editors,
            secondary_editors,
            primary_active_editor,
            secondary_active_editor,
            pinned_editors,
        }
    }
}

/// Drop empty keys and duplicates, keeping first-occurrence strip order.
fn normalize_keys(keys: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::with_capacity(keys.len());
    for key in keys {
        if !key.is_empty() && !normalized.contains(key) {
            normalized.push(key.clone());
        }
    }
    normalized
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Same-directory rename preserves the previous complete snapshot if the
    // process terminates while writing the replacement.
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = path.with_extension(format!("tmp-{}-{suffix}", std::process::id()));
    let write_result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample() -> SavedWorkbenchLayoutV1 {
        SavedWorkbenchLayoutV1 {
            version: WORKBENCH_LAYOUT_VERSION,
            split_enabled: true,
            axis: SavedWorkbenchAxis::Vertical,
            focused_pane: SavedWorkbenchPane::Secondary,
            primary_editors: vec!["editor:7:1".into(), "editor:7:2".into()],
            secondary_editors: vec!["editor:8:3".into()],
            primary_active_editor: Some("editor:7:2".into()),
            secondary_active_editor: Some("editor:8:3".into()),
            pinned_editors: Vec::new(),
        }
    }

    #[test]
    fn scratch_layout_key_uses_only_bounded_stable_identity_not_workspace_path() {
        let id = "a".repeat(56);
        let key = scratch_layout_key(&id).expect("valid recovered draft id");
        assert_eq!(scratch_identity_from_layout_key(&key), Some(id.as_str()));
        assert!(scratch_layout_key("../escape").is_none());
        assert!(scratch_layout_key(&"a".repeat(57)).is_none());
        assert!(scratch_identity_from_layout_key("file.txt").is_none());
        assert!(scratch_identity_from_layout_key("@alera-scratch:wrong").is_none());

        let mut layout = sample();
        layout.secondary_editors = vec![key.clone()];
        layout.secondary_active_editor = Some(key.clone());
        let restored = SavedWorkbenchLayoutV1::from_json(&layout.to_json().unwrap()).unwrap();
        assert_eq!(restored.secondary_editors, vec![key.clone()]);
        assert_eq!(
            restored.secondary_active_editor.as_deref(),
            Some(key.as_str())
        );
    }

    #[test]
    fn json_roundtrip_preserves_layout_and_schema_names() {
        let saved = sample();
        let json = saved.to_json().unwrap();
        assert_eq!(
            json,
            concat!(
                "{\"version\":1,\"split_enabled\":true,\"axis\":\"vertical\",",
                "\"focused_pane\":\"secondary\",",
                "\"primary_editors\":[\"editor:7:1\",\"editor:7:2\"],",
                "\"secondary_editors\":[\"editor:8:3\"],",
                "\"primary_active_editor\":\"editor:7:2\",",
                "\"secondary_active_editor\":\"editor:8:3\"}"
            )
        );
        assert_eq!(SavedWorkbenchLayoutV1::from_json(&json).unwrap(), saved);
    }

    #[test]
    fn disk_roundtrip_saves_and_loads_the_same_layout() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("nested").join("layout.json");
        let saved = sample();
        saved.save_to_path(&path).unwrap();
        let loaded = SavedWorkbenchLayoutV1::load_from_path(&path)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, saved);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\":1"));
        assert!(text.contains("\"primary_editors\":[\"editor:7:1\""));
    }

    #[test]
    fn old_layout_is_readable_and_pinned_keys_are_pane_scoped() {
        let old_json = sample().to_json().unwrap();
        assert!(!old_json.contains("pinned_editors"));
        assert!(SavedWorkbenchLayoutV1::from_json(&old_json)
            .unwrap()
            .pinned_editors
            .is_empty());
        let mut saved = sample();
        saved.pinned_editors = vec![
            "editor:7:2".into(),
            "editor:7:2".into(),
            "editor:8:3".into(),
            "outside-pane".into(),
            String::new(),
        ];
        let normalized = SavedWorkbenchLayoutV1::from_json(&saved.to_json().unwrap()).unwrap();
        assert_eq!(normalized.pinned_editors, vec!["editor:7:2", "editor:8:3"]);
        assert!(normalized.to_json().unwrap().contains("pinned_editors"));
    }

    #[test]
    fn pinned_editor_moved_between_panes_survives_restart_and_drops_closed_pin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workbench-layout.json");
        let mut state = sample();
        state.pinned_editors = vec!["editor:7:1".into(), "editor:8:3".into()];
        state.primary_editors.retain(|key| key != "editor:7:1");
        state.secondary_editors.insert(0, "editor:7:1".into());
        state.save_to_path(&path).unwrap();
        let restored = SavedWorkbenchLayoutV1::load_from_path(&path)
            .unwrap()
            .expect("layout persisted");
        assert_eq!(restored.secondary_editors, vec!["editor:7:1", "editor:8:3"]);
        assert_eq!(restored.pinned_editors, vec!["editor:7:1", "editor:8:3"]);
        assert_eq!(
            restored.primary_active_editor.as_deref(),
            Some("editor:7:2")
        );

        let mut closed = restored;
        closed.secondary_editors.retain(|key| key != "editor:7:1");
        closed.pinned_editors.push("editor:7:1".into());
        closed.save_to_path(&path).unwrap();
        let without_closed = SavedWorkbenchLayoutV1::load_from_path(&path)
            .unwrap()
            .expect("layout still persisted");
        assert_eq!(without_closed.pinned_editors, vec!["editor:8:3"]);
        assert_eq!(without_closed.secondary_editors, vec!["editor:8:3"]);
    }

    #[test]
    fn repeated_save_replaces_existing_layout_atomically_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.json");
        let mut saved = sample();
        saved.save_to_path(&path).unwrap();
        saved.axis = SavedWorkbenchAxis::Horizontal;
        saved.primary_editors = vec!["editor:7:2".into()];
        saved.primary_active_editor = Some("editor:7:2".into());
        saved.save_to_path(&path).unwrap();
        assert_eq!(
            SavedWorkbenchLayoutV1::load_from_path(&path).unwrap(),
            Some(saved)
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn unknown_version_is_rejected_on_load_and_write() {
        let saved = sample();
        for version in [0_u32, 2, u32::MAX] {
            let mut value: serde_json::Value =
                serde_json::from_str(&saved.to_json().unwrap()).unwrap();
            value["version"] = serde_json::json!(version);
            let text = value.to_string();
            assert!(
                SavedWorkbenchLayoutV1::from_json(&text).is_err(),
                "version {version} must be rejected"
            );
        }
        let mut stamped = saved.clone();
        stamped.version = WORKBENCH_LAYOUT_VERSION + 1;
        assert!(stamped.to_json().is_err());
        let workspace = tempfile::tempdir().unwrap();
        assert!(stamped
            .save_to_path(&workspace.path().join("layout.json"))
            .is_err());
    }

    #[test]
    fn malformed_or_incomplete_json_is_rejected() {
        for text in [
            "",
            "not json",
            "{",
            "[]",
            "null",
            "\"layout\"",
            "42",
            "{\"version\":1}",
            "{\"version\":\"1\"}",
        ] {
            assert!(
                SavedWorkbenchLayoutV1::from_json(text).is_err(),
                "{text:?} must be rejected"
            );
        }
        let mut value: serde_json::Value =
            serde_json::from_str(&sample().to_json().unwrap()).unwrap();
        value["axis"] = serde_json::json!("diagonal");
        assert!(SavedWorkbenchLayoutV1::from_json(&value.to_string()).is_err());
        value["axis"] = serde_json::json!("horizontal");
        value["focused_pane"] = serde_json::json!("middle");
        assert!(SavedWorkbenchLayoutV1::from_json(&value.to_string()).is_err());
    }

    #[test]
    fn duplicate_and_empty_keys_are_normalized() {
        let text = concat!(
            "{\"version\":1,\"split_enabled\":true,\"axis\":\"horizontal\",",
            "\"focused_pane\":\"secondary\",",
            "\"primary_editors\":[\"a\",\"\",\"a\",\"b\",\"a\"],",
            "\"secondary_editors\":[\"a\",\"\",\"b\",\"c\",\"c\"],",
            "\"primary_active_editor\":\"a\",",
            "\"secondary_active_editor\":\"b\"}"
        );
        let saved = SavedWorkbenchLayoutV1::from_json(text).unwrap();
        assert_eq!(saved.primary_editors, ["a", "b"]);
        // "a" and "b" already live in the primary pane; "c" dedupes to one.
        assert_eq!(saved.secondary_editors, ["c"]);
        assert_eq!(saved.primary_active_editor.as_deref(), Some("a"));
        // "b" is not a secondary editor anymore, so the stale active clears.
        assert!(saved.secondary_active_editor.is_none());
        assert_eq!(saved.focused_pane, SavedWorkbenchPane::Secondary);
        assert!(saved.split_enabled);
        let canonical = SavedWorkbenchLayoutV1::from_json(&saved.to_json().unwrap()).unwrap();
        assert_eq!(canonical, saved);

        // A secondary pane emptied by normalization cannot keep focus or an
        // active editor; scalar flags are preserved verbatim.
        let text = concat!(
            "{\"version\":1,\"split_enabled\":false,\"axis\":\"vertical\",",
            "\"focused_pane\":\"secondary\",",
            "\"primary_editors\":[\"x\"],",
            "\"secondary_editors\":[\"\",\"x\",\"x\"],",
            "\"primary_active_editor\":\"ghost\",\"secondary_active_editor\":\"\"}"
        );
        let saved = SavedWorkbenchLayoutV1::from_json(text).unwrap();
        assert!(saved.secondary_editors.is_empty());
        assert!(saved.secondary_active_editor.is_none());
        assert!(saved.primary_active_editor.is_none());
        assert_eq!(saved.focused_pane, SavedWorkbenchPane::Primary);
        assert!(!saved.split_enabled);
        assert_eq!(saved.axis, SavedWorkbenchAxis::Vertical);
    }

    #[test]
    fn missing_file_loads_as_none() {
        let workspace = tempfile::tempdir().unwrap();
        let absent = workspace.path().join("nested").join("layout.json");
        assert!(SavedWorkbenchLayoutV1::load_from_path(&absent)
            .unwrap()
            .is_none());
        let file = workspace.path().join("layout.json");
        assert!(SavedWorkbenchLayoutV1::load_from_path(&file)
            .unwrap()
            .is_none());
    }

    #[test]
    fn unknown_fields_and_host_handles_are_never_round_tripped() {
        let mut value: serde_json::Value =
            serde_json::from_str(&sample().to_json().unwrap()).unwrap();
        value["terminal_handles"] = serde_json::json!(["pty-1", "pty-agent"]);
        value["agent_handles"] = serde_json::json!(["agent-1"]);
        value["future_extension"] = serde_json::json!({"nested": true});
        let saved = SavedWorkbenchLayoutV1::from_json(&value.to_string()).unwrap();
        assert_eq!(saved, sample());
        let emitted = saved.to_json().unwrap();
        assert!(!emitted.contains("pty-1"));
        assert!(!emitted.contains("agent-1"));
        assert!(!emitted.contains("future_extension"));
    }
}
