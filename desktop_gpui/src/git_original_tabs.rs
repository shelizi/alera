//! Immutable Git original tabs. These deliberately never own EditorDocument,
//! EditorSession, a worktree disk path for save, draft storage, or undo actions.
//! Closing/focusing these tabs cannot save, mutate or discard editor buffers.
use crate::git_inspect::{GitChangeArea, GitOriginalSnapshot};

pub const MAX_GIT_ORIGINAL_TABS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOriginalTab {
    pub instance: u64,
    pub snapshot: GitOriginalSnapshot,
}

impl GitOriginalTab {
    pub fn title(&self) -> String {
        self.snapshot.title()
    }

    pub fn description(&self) -> String {
        format!(
            "{} · {} · immutable Git blob {}",
            self.snapshot.relative.display(),
            match self.snapshot.area {
                GitChangeArea::Staged => "HEAD before staging",
                GitChangeArea::Changes => "Index before working changes",
                _ => "Unknown revision",
            },
            self.snapshot.blob_oid
        )
    }
}

#[derive(Debug, Default)]
pub struct GitOriginalTabs {
    tabs: Vec<GitOriginalTab>,
    active: Option<u64>,
    next_instance: u64,
}

impl GitOriginalTabs {
    pub fn entries(&self) -> &[GitOriginalTab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<&GitOriginalTab> {
        self.active
            .and_then(|id| self.tabs.iter().find(|tab| tab.instance == id))
    }

    pub fn open(&mut self, snapshot: GitOriginalSnapshot) -> Result<u64, String> {
        if !matches!(
            snapshot.area,
            GitChangeArea::Staged | GitChangeArea::Changes
        ) {
            return Err(
                "Only an original HEAD or index Git blob may open as a read-only tab".into(),
            );
        }
        if snapshot.blob_oid.len() != 40
            || !snapshot.blob_oid.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid original Git blob identity".into());
        }
        if let Some(existing) = self.tabs.iter().find(|tab| {
            tab.snapshot.relative == snapshot.relative
                && tab.snapshot.area == snapshot.area
                && tab.snapshot.blob_oid == snapshot.blob_oid
        }) {
            self.active = Some(existing.instance);
            return Ok(existing.instance);
        }
        if self.tabs.len() >= MAX_GIT_ORIGINAL_TABS {
            return Err("Close an older Git original tab before opening another (limit 16)".into());
        }
        self.next_instance = self.next_instance.wrapping_add(1);
        if self.next_instance == 0 {
            self.next_instance = 1;
        }
        let id = self.next_instance;
        self.tabs.push(GitOriginalTab {
            instance: id,
            snapshot,
        });
        self.active = Some(id);
        Ok(id)
    }

    pub fn activate(&mut self, id: u64) -> bool {
        if self.tabs.iter().any(|tab| tab.instance == id) {
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    pub fn close(&mut self, id: u64) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.instance == id) else {
            return false;
        };
        self.tabs.remove(index);
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.instance);
        }
        true
    }

    /// Scratch and file-backed native editor tabs are owned elsewhere.
    /// Git original snapshots are ephemeral and never persisted/recovered.
    pub fn reset(&mut self) {
        self.tabs.clear();
        self.active = None;
        // Do not reuse stale tab identities within the same Workbench.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn snapshot(area: GitChangeArea, oid: &str) -> GitOriginalSnapshot {
        GitOriginalSnapshot {
            relative: PathBuf::from("src/file.rs"),
            area,
            blob_oid: oid.to_owned(),
            content: "immutable snapshot".into(),
        }
    }
    #[test]
    fn original_tabs_keep_head_and_index_distinct_without_mutable_documents() {
        let mut tabs = GitOriginalTabs::default();
        let id = tabs
            .open(snapshot(GitChangeArea::Staged, &"a".repeat(40)))
            .unwrap();
        assert_eq!(
            tabs.open(snapshot(GitChangeArea::Staged, &"a".repeat(40)))
                .unwrap(),
            id
        );
        let second = tabs
            .open(snapshot(GitChangeArea::Changes, &"b".repeat(40)))
            .unwrap();
        assert_ne!(id, second);
        assert_eq!(tabs.entries().len(), 2);
        assert!(tabs.entries()[0].description().contains("HEAD"));
        assert!(tabs.entries()[1].description().contains("Index"));
        assert!(tabs.activate(id));
        assert!(tabs.close(id));
        assert_eq!(tabs.active().unwrap().instance, second);
        assert!(!tabs.activate(id));
        tabs.reset();
        assert!(tabs.active().is_none());
    }
    #[test]
    fn rejects_nonoriginal_invalid_ids_and_bounded_overflow() {
        let mut tabs = GitOriginalTabs::default();
        assert!(tabs
            .open(snapshot(GitChangeArea::Untracked, &"a".repeat(40)))
            .is_err());
        assert!(tabs
            .open(snapshot(GitChangeArea::Staged, "invalid"))
            .is_err());
        for id in 1..=MAX_GIT_ORIGINAL_TABS {
            let oid = format!("{id:040x}");
            assert!(tabs.open(snapshot(GitChangeArea::Staged, &oid)).is_ok());
        }
        assert!(tabs
            .open(snapshot(GitChangeArea::Staged, &"f".repeat(40)))
            .is_err());
        assert_eq!(tabs.entries().len(), MAX_GIT_ORIGINAL_TABS);
    }
}
