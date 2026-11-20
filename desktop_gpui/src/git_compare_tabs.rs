//! Immutable, workspace-local Git file comparison tabs. A comparison is an
//! already-computed snapshot, never an EditorDocument, disk save path or PTY.
//! In particular, closing a comparison cannot stage, checkout, or discard.
use crate::git_file_revision_compare::GitFileRevisionComparison;
use crate::git_file_split_diff::{
    align_git_file_patch, flatten_full_file_rows, GitDiffCell, GitSplitDiffRow,
};
use std::sync::Arc;

pub const MAX_GIT_COMPARE_TABS: usize = 8;

#[derive(Clone, Debug)]
pub struct GitFileCompareTab {
    pub instance: u64,
    pub comparison: GitFileRevisionComparison,
    pub split: bool,
    pub full: bool,
    pub diff_rows: Arc<Vec<GitSplitDiffRow>>,
    pub full_rows: Arc<Vec<GitSplitDiffRow>>,
    pub full_unified_rows: Arc<Vec<GitDiffCell>>,
    pub full_error: Option<String>,
    pub additions: usize,
    pub deletions: usize,
    pub diff_clipped: bool,
}

impl GitFileCompareTab {
    pub fn title(&self) -> String {
        let filename = self
            .comparison
            .relative
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| self.comparison.relative.display().to_string());
        format!("{filename} ↔ {}", self.comparison.reference)
    }
    pub fn full_available(&self) -> bool {
        self.full_error.is_none()
    }
    pub fn displayed_full(&self) -> bool {
        self.full && self.full_available()
    }
    pub fn set_full(&mut self, full: bool) -> bool {
        if full && !self.full_available() || self.full == full {
            return false;
        }
        self.full = full;
        true
    }
}

#[derive(Debug, Default)]
pub struct GitFileCompareTabs {
    tabs: Vec<GitFileCompareTab>,
    next_instance: u64,
}

impl GitFileCompareTabs {
    pub fn entries(&self) -> &[GitFileCompareTab] {
        &self.tabs
    }
    pub fn get(&self, instance: u64) -> Option<&GitFileCompareTab> {
        self.tabs.iter().find(|tab| tab.instance == instance)
    }
    pub fn get_mut(&mut self, instance: u64) -> Option<&mut GitFileCompareTab> {
        self.tabs.iter_mut().find(|tab| tab.instance == instance)
    }
    /// Reopening an identical immutable snapshot focuses its existing
    /// identity; a changed worktree patch gets its own distinct comparison.
    pub fn open(&mut self, comparison: GitFileRevisionComparison) -> Result<u64, String> {
        if let Some(existing) = self.tabs.iter().find(|tab| {
            tab.comparison.relative == comparison.relative
                && tab.comparison.reference == comparison.reference
                && tab.comparison.revision_oid == comparison.revision_oid
                && tab.comparison.patch == comparison.patch
        }) {
            return Ok(existing.instance);
        }
        if self.tabs.len() >= MAX_GIT_COMPARE_TABS {
            return Err(
                "Close an older Git comparison tab before opening another (limit 8)".into(),
            );
        }
        let aligned = align_git_file_patch(&comparison.patch);
        let (full_rows, full_unified_rows, full_error) = match &comparison.full_file {
            Ok(full) => (
                Arc::new(full.rows.clone()),
                Arc::new(flatten_full_file_rows(full)),
                None,
            ),
            Err(error) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Some(error.clone()),
            ),
        };
        self.next_instance = self.next_instance.wrapping_add(1);
        if self.next_instance == 0 {
            self.next_instance = 1;
        }
        let id = self.next_instance;
        self.tabs.push(GitFileCompareTab {
            instance: id,
            split: true,
            full: full_error.is_none(),
            comparison,
            additions: aligned.additions,
            deletions: aligned.deletions,
            diff_clipped: aligned.clipped,
            diff_rows: Arc::new(aligned.rows),
            full_rows,
            full_unified_rows,
            full_error,
        });
        Ok(id)
    }
    pub fn close(&mut self, instance: u64) -> bool {
        let len = self.tabs.len();
        self.tabs.retain(|tab| tab.instance != instance);
        self.tabs.len() != len
    }
    pub fn reset(&mut self) {
        self.tabs.clear();
        // No stale tab identity may bind to a different comparison later.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_file_revision_compare::compare_file_with_git_reference;
    use crate::git_file_split_diff::expand_git_patch_to_full_file;
    use crate::workbench_panes::{PaneSlot, WorkbenchPaneLayout};
    use crate::workbench_tabs::{
        WorkbenchTab, WorkbenchTabIdentity, WorkbenchTabKind, WorkbenchTabs,
    };
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    fn revision(path: &str, new_text: &str) -> GitFileRevisionComparison {
        let patch = format!("@@ -1,2 +1,2 @@\n before\n-old\n+{new_text}\n");
        GitFileRevisionComparison {
            relative: PathBuf::from(path),
            reference: "HEAD".into(),
            revision_oid: "a".repeat(40),
            full_file: expand_git_patch_to_full_file(
                &patch,
                "before\nold\n",
                &format!("before\n{new_text}\n"),
            ),
            patch,
        }
    }

    #[test]
    fn reopen_identical_snapshot_deduplicates_and_changed_snapshot_is_distinct() {
        let mut tabs = GitFileCompareTabs::default();
        let first = tabs.open(revision("src/hello.rs", "new")).unwrap();
        assert_eq!(first, tabs.open(revision("src/hello.rs", "new")).unwrap());
        let second = tabs.open(revision("src/hello.rs", "newer")).unwrap();
        assert_ne!(first, second);
        assert_eq!(tabs.entries().len(), 2);
        assert_eq!(tabs.get(first).unwrap().title(), "hello.rs ↔ HEAD");
        assert_eq!(tabs.get(first).unwrap().full_rows.len(), 2);
        assert_eq!(tabs.get(first).unwrap().additions, 1);
        assert_eq!(tabs.get(second).unwrap().full_unified_rows.len(), 3);
        assert!(tabs.get_mut(first).unwrap().set_full(false));
        assert!(!tabs.get_mut(first).unwrap().set_full(false));
        assert!(tabs.close(first));
        assert!(!tabs.close(first));
        assert!(tabs.get(first).is_none());
        tabs.reset();
        let third = tabs.open(revision("src/hello.rs", "new")).unwrap();
        assert_ne!(first, third);
    }

    #[test]
    fn unavailable_full_file_preserves_diff_only_and_refuses_full_toggle() {
        let mut tabs = GitFileCompareTabs::default();
        let mut diff = revision("doc.txt", "new");
        diff.full_file = Err("Full file too large".into());
        let id = tabs.open(diff).unwrap();
        let tab = tabs.get_mut(id).unwrap();
        assert!(!tab.full_available());
        assert!(!tab.displayed_full());
        assert!(!tab.set_full(true));
        assert_eq!(tab.diff_rows.len(), 3);
        assert_eq!(tab.full_error.as_deref(), Some("Full file too large"));
    }

    #[test]
    fn limit_is_bounded_and_failed_open_preserves_existing_tabs() {
        let mut tabs = GitFileCompareTabs::default();
        for n in 0..MAX_GIT_COMPARE_TABS {
            tabs.open(revision(&format!("src/{n}.rs"), "new")).unwrap();
        }
        assert!(tabs.open(revision("src/ninth.rs", "new")).is_err());
        assert_eq!(tabs.entries().len(), MAX_GIT_COMPARE_TABS);
    }

    #[test]
    fn immutable_git_snapshot_survives_worktree_changes_without_writing_head_or_index() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        let path = tmp.path().join("a.txt");
        fs::write(&path, "initial\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        index.write().unwrap();
        let sig = git2::Signature::now("Tab QA", "qa@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        drop(tree);
        drop(index);
        let head_before = repo.head().unwrap().target();
        let index_before = fs::read(repo.path().join("index")).unwrap();
        fs::write(&path, "before pin\n").unwrap();
        let captured =
            compare_file_with_git_reference(tmp.path(), Path::new("a.txt"), None).unwrap();
        let mut tabs = GitFileCompareTabs::default();
        let id = tabs.open(captured).unwrap();
        fs::write(&path, "changed after pin\n").unwrap();
        assert!(tabs
            .get(id)
            .unwrap()
            .comparison
            .patch
            .contains("+before pin"));
        assert!(!tabs
            .get(id)
            .unwrap()
            .comparison
            .patch
            .contains("changed after pin"));
        assert_eq!(
            tabs.get(id).unwrap().full_rows[0]
                .new
                .as_ref()
                .unwrap()
                .text,
            "before pin"
        );
        assert_eq!(fs::read(&path).unwrap(), b"changed after pin\n");
        assert_eq!(fs::read(repo.path().join("index")).unwrap(), index_before);
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert!(tabs.close(id));
        assert_eq!(fs::read(&path).unwrap(), b"changed after pin\n");
    }

    #[test]
    fn pinned_compare_mixed_pane_move_close_and_workspace_reset_never_attach_a_host() {
        let mut store = GitFileCompareTabs::default();
        let instance = store.open(revision("src/hello.rs", "new")).unwrap();
        let id = WorkbenchTabIdentity::GitFileCompare(instance);
        let view = WorkbenchTab {
            identity: id.clone(),
            kind: WorkbenchTabKind::GitFileCompare,
            title: store.get(instance).unwrap().title(),
            dirty: false,
            running: true,
            agent_state: None,
            interrupted: false,
        };
        let mut tabs = WorkbenchTabs::for_workspace(7);
        tabs.reconcile(7, vec![view.clone()], None);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[id.clone()]);
        assert!(tabs.set_pinned(&id, true));
        assert!(panes.move_to(&id, PaneSlot::Secondary));
        assert!(panes.has_secondary());
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&id));
        assert!(tabs.is_pinned(&id));
        assert!(panes.move_to(&id, PaneSlot::Primary));
        assert!(!panes.has_secondary());
        store.close(instance);
        tabs.reconcile(7, vec![], None);
        panes.reconcile(&[]);
        assert!(!tabs.contains(&id));
        assert!(!tabs.is_pinned(&id));
        assert!(panes.active_in(PaneSlot::Primary).is_none());
        store.reset();
    }
}
