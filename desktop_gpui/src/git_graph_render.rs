//! Immutable, bounded projection for the central Git Graph viewport.
//! Build topology once per history snapshot, then draw only visible rows.
//! Searching retains full-history lane positions to avoid fabricated edges.

use crate::git_graph_layout::{layout_git_graph, GraphCommit, GraphLaneRow};
use crate::git_graph_search::GitGraphSearch;
use crate::git_history::{
    GitCommitRow, GitHistorySnapshot, GitMergedBranchVisibility, MAX_HISTORY_PAGE,
};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct PreparedGitGraphRow {
    pub commit: GitCommitRow,
    pub labels: Vec<String>,
    pub topology: Option<GraphLaneRow>,
    pub is_head: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedGitGraph {
    pub rows: Vec<PreparedGitGraphRow>,
    pub lane_count: usize,
    pub topology_error: Option<String>,
}

impl PreparedGitGraph {
    pub fn new(
        history: &GitHistorySnapshot,
        visibility: GitMergedBranchVisibility,
        sparse_search: bool,
    ) -> Self {
        let commits = history.display_commits(visibility);
        let layout = if sparse_search {
            None
        } else {
            Some(layout_git_graph(
                &commits
                    .iter()
                    .map(|row| GraphCommit {
                        oid: row.oid.clone(),
                        parents: row.parent_ids.clone(),
                    })
                    .collect::<Vec<_>>(),
            ))
        };
        let (rails, lane_count, topology_error) = match layout {
            Some(Ok(graph)) => (Some(graph.rows), graph.max_lanes, None),
            Some(Err(error)) => (None, 1, Some(error.message().to_owned())),
            None => (None, 1, None),
        };
        let head_tip = history
            .branches
            .iter()
            .find(|branch| branch.current)
            .map(|branch| branch.tip.as_str());
        let mut tags_by_oid: HashMap<&str, Vec<&str>> = HashMap::new();
        for tag in &history.tags {
            tags_by_oid
                .entry(tag.oid.as_str())
                .or_default()
                .push(tag.name.as_str());
        }
        let rows = commits
            .into_iter()
            .enumerate()
            .map(|(index, commit)| {
                let mut labels = commit.labels.clone();
                if let Some(tags) = tags_by_oid.get(commit.oid.as_str()) {
                    labels.extend(tags.iter().map(|name| format!("refs/tags/{name}")));
                }
                labels.sort_by_key(|label| {
                    if label == &history.current {
                        0
                    } else if label.starts_with("refs/tags/") {
                        1
                    } else if label.starts_with("refs/remotes/") {
                        3
                    } else {
                        2
                    }
                });
                let is_head = head_tip.is_some_and(|tip| tip == commit.short);
                PreparedGitGraphRow {
                    commit,
                    labels,
                    topology: rails.as_ref().and_then(|rows| rows.get(index).cloned()),
                    is_head,
                }
            })
            .take(MAX_HISTORY_PAGE)
            .collect();
        Self {
            rows,
            lane_count,
            topology_error,
        }
    }

    /// Return original unfiltered indices; sparse matches do NOT rewire rails.
    pub fn matching_indices(&self, query: &GitGraphSearch, sparse_search: bool) -> Vec<usize> {
        // Empty filter and sparse global search need no string matching or
        // per-commit temporary label arrays on each GPUI render.
        if sparse_search || !query.is_active() {
            return (0..self.rows.len().min(MAX_HISTORY_PAGE)).collect();
        }
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (sparse_search || query.commit(&row.commit)).then_some(index)
            })
            .take(MAX_HISTORY_PAGE)
            .collect()
    }

    /// Read only the window supplied by GPUI's uniform_list, even if it spans
    /// the end of the 512-row page. This mapping never creates UI elements.
    pub fn viewport_indices<'a>(matches: &'a [usize], range: Range<usize>) -> &'a [usize] {
        let start = range.start.min(matches.len());
        let end = range.end.min(matches.len()).max(start);
        &matches[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_history::{GitBranchRow, GitMergedBranchVisibility as Visibility};

    fn fixture(count: usize) -> GitHistorySnapshot {
        let commits = (0..count)
            .rev()
            .map(|i| GitCommitRow {
                oid: format!("commit-{i:04}"),
                short: format!("{i:04}"),
                subject: if i == 1 {
                    "needle unique".into()
                } else {
                    format!("commit {i}")
                },
                author: "Alera tester".into(),
                parents: usize::from(i > 0),
                parent_ids: if i > 0 {
                    vec![format!("commit-{:04}", i - 1)]
                } else {
                    vec![]
                },
                labels: if i == count - 1 {
                    vec!["main".into()]
                } else {
                    vec![]
                },
            })
            .collect();
        GitHistorySnapshot {
            current: "main".into(),
            branches: vec![GitBranchRow {
                name: "main".into(),
                current: true,
                remote: false,
                favorite: false,
                tip: format!("{:04}", count.saturating_sub(1)),
            }],
            tags: vec![],
            commits,
            truncated: false,
            scanned_commits: count,
            scan_limit_reached: false,
            merged_branch_refs: vec![],
            merged_target_oid: None,
        }
    }

    #[test]
    fn complete_512_commit_page_retains_original_parent_lanes() {
        let prepared = PreparedGitGraph::new(&fixture(512), Visibility::ShowAll, false);
        assert_eq!(prepared.rows.len(), 512);
        assert_eq!(prepared.lane_count, 1);
        assert!(prepared.topology_error.is_none());
        assert!(prepared.rows[0].is_head);
        assert!(prepared.rows[0].topology.as_ref().unwrap().has_incoming == false);
        assert!(prepared.rows[511]
            .topology
            .as_ref()
            .unwrap()
            .edges
            .is_empty());
        let matches = prepared.matching_indices(&GitGraphSearch::parse(""), false);
        assert_eq!(matches.len(), 512);
        assert_eq!(
            PreparedGitGraph::viewport_indices(&matches, 0..18).len(),
            18
        );
        assert_eq!(
            PreparedGitGraph::viewport_indices(&matches, 500..525),
            &(500..512).collect::<Vec<_>>()
        );
        assert!(PreparedGitGraph::viewport_indices(&matches, 512..600).is_empty());
    }

    #[test]
    fn filtered_match_uses_original_topology_not_adjacent_filtered_commit() {
        let prepared = PreparedGitGraph::new(&fixture(512), Visibility::ShowAll, false);
        let matches = prepared.matching_indices(&GitGraphSearch::parse("message:needle"), false);
        assert_eq!(matches, vec![510]);
        assert_eq!(prepared.rows[matches[0]].commit.oid, "commit-0001");
        let row = prepared.rows[matches[0]].topology.as_ref().unwrap();
        assert_eq!(row.oid, "commit-0001");
        assert!(!row.edges.is_empty());
        // A sparse global search intentionally never shows incomplete rails.
        let sparse = PreparedGitGraph::new(&fixture(512), Visibility::ShowAll, true);
        assert!(sparse.rows.iter().all(|row| row.topology.is_none()));
        assert_eq!(
            sparse
                .matching_indices(&GitGraphSearch::parse("needle"), true)
                .len(),
            512
        );
    }

    #[test]
    fn central_graph_uses_virtualized_rows_not_eager_commit_children() {
        let center = include_str!("native_git_graph_center_ui.rs");
        assert!(center.contains("gpui::uniform_list("));
        assert!(center.contains("PreparedGitGraph::viewport_indices"));
        assert!(!center.contains(".children(rows)"));
        let rows = include_str!("native_git_graph_rows_ui.rs");
        assert!(rows.contains("graph_lane_with_density"));
    }

    #[test]
    fn real_git_history_513_commits_loads_and_virtualizes_bounded_512_page() {
        use git2::{Repository, Signature};
        use std::path::Path;
        use std::time::Instant;

        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        let signature = Signature::now("Alera", "test@example.invalid").unwrap();
        for i in 0..513 {
            std::fs::write(dir.path().join("timeline.txt"), format!("commit {i}\n")).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("timeline.txt")).unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
            let parents = parent.iter().collect::<Vec<_>>();
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("history {i}"),
                &tree,
                &parents,
            )
            .unwrap();
            index.write().unwrap();
        }
        let options = crate::git_history::GitHistoryOptions {
            max_commits: 512,
            ..Default::default()
        };
        let start = Instant::now();
        let history = GitHistorySnapshot::load_filtered(dir.path(), &options).unwrap();
        let load_time = start.elapsed();
        assert_eq!(history.commits.len(), 512);
        let start = Instant::now();
        let prepared = PreparedGitGraph::new(&history, Visibility::ShowAll, false);
        let prep_time = start.elapsed();
        assert_eq!(prepared.rows.len(), 512);
        assert!(prepared.topology_error.is_none());
        let matches = prepared.matching_indices(&GitGraphSearch::parse(""), false);
        assert_eq!(
            PreparedGitGraph::viewport_indices(&matches, 0..20).len(),
            20
        );
        assert_eq!(
            PreparedGitGraph::viewport_indices(&matches, 495..512).len(),
            17
        );
        println!(
            "512 Git history page: load={load_time:?}, cached topology={prep_time:?}, visible rows=20/512"
        );
    }

    #[test]
    fn empty_graph_and_out_of_range_windows_do_not_panic() {
        let prepared = PreparedGitGraph::new(&fixture(0), Visibility::ShowAll, false);
        assert!(prepared.rows.is_empty());
        assert!(prepared
            .matching_indices(&GitGraphSearch::parse(""), false)
            .is_empty());
        assert!(PreparedGitGraph::viewport_indices(&[], 100..999).is_empty());
    }
}
