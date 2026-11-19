//! Regression tests for immutable branch-tip and merge-base comparisons.

use super::*;
use git2::{Repository, Signature};

fn commit_file(repo: &Repository, root: &Path, path: &str, contents: &str, title: &str) {
    std::fs::write(root.join(path), contents).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let author = Signature::now("Test", "test@example.invalid").unwrap();
    if let Some(parent) = &parent {
        repo.commit(Some("HEAD"), &author, &author, title, &tree, &[parent])
            .unwrap();
    } else {
        repo.commit(Some("HEAD"), &author, &author, title, &tree, &[])
            .unwrap();
    }
    index.write().unwrap();
}

#[test]
fn directional_comparison_only_reads_committed_tree_contents() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    commit_file(&repo, dir.path(), "shared.txt", "base\n", "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let current_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    repo.branch("topic", &base, false).unwrap();
    drop(base);
    commit_file(
        &repo,
        dir.path(),
        "shared.txt",
        "current\n",
        "current change",
    );
    let current_id = repo.head().unwrap().target().unwrap();
    repo.set_head("refs/heads/topic").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    commit_file(&repo, dir.path(), "shared.txt", "topic\n", "topic change");
    let topic_id = repo.head().unwrap().target().unwrap();
    repo.set_head(&format!("refs/heads/{current_name}"))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    std::fs::write(dir.path().join("shared.txt"), "uncommitted secret\n").unwrap();
    let current_to_selected =
        compare_branch_files(dir.path(), "topic", CompareDirection::CurrentToSelected).unwrap();
    assert_eq!(current_to_selected.target_oid, topic_id.to_string());
    assert_eq!(current_to_selected.files.len(), 1);
    assert_eq!(current_to_selected.ahead, 1);
    assert_eq!(current_to_selected.behind, 1);
    assert!(diff_compared_file(dir.path(), &current_to_selected, 0)
        .unwrap()
        .contains("+topic"));
    assert!(!diff_compared_file(dir.path(), &current_to_selected, 0)
        .unwrap()
        .contains("uncommitted secret"));
    assert!(original_compared_file(dir.path(), &current_to_selected, 0)
        .unwrap()
        .contains("base\n"));
    let snap = open_compared_original_snapshot(dir.path(), &current_to_selected, 0).unwrap();
    assert_eq!(snap.content, "base\n");
    assert!(snap
        .revision_label
        .as_deref()
        .unwrap_or_default()
        .contains("MERGE BASE"));
    let mut originals = crate::git_original_tabs::GitOriginalTabs::default();
    originals.open(snap).unwrap();
    assert!(originals.active().unwrap().title().contains("MERGE BASE"));
    let selected_to_current =
        compare_branch_files(dir.path(), "topic", CompareDirection::SelectedToCurrent).unwrap();
    assert_eq!(selected_to_current.target_oid, current_id.to_string());
    assert!(diff_compared_file(dir.path(), &selected_to_current, 0)
        .unwrap()
        .contains("+current"));
    assert_eq!(repo.head().unwrap().target().unwrap(), current_id);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "uncommitted secret\n"
    );
}

#[test]
fn full_tip_comparison_reverses_additions_deletions_and_file_content() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    commit_file(&repo, dir.path(), "shared.txt", "base\n", "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let current_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    repo.branch("topic", &base, false).unwrap();
    drop(base);
    commit_file(&repo, dir.path(), "shared.txt", "current\n", "current");
    commit_file(
        &repo,
        dir.path(),
        "current-only.txt",
        "exclusive current\n",
        "current only",
    );
    let current_oid = repo.head().unwrap().target().unwrap();
    repo.set_head("refs/heads/topic").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    commit_file(&repo, dir.path(), "shared.txt", "topic\n", "topic");
    commit_file(
        &repo,
        dir.path(),
        "topic-only.txt",
        "exclusive topic\n",
        "topic only",
    );
    let topic_oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/topic", topic_oid, true, "test remote")
        .unwrap();
    repo.set_head(&format!("refs/heads/{current_name}"))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    std::fs::write(dir.path().join("shared.txt"), "uncommitted secret\n").unwrap();
    let forward =
        compare_branch_trees(dir.path(), "topic", CompareDirection::CurrentToSelected).unwrap();
    assert_eq!(forward.basis, ComparisonBasis::BranchTips);
    assert_eq!(forward.source_oid, current_oid.to_string());
    assert_eq!(forward.target_oid, topic_oid.to_string());
    assert_eq!(forward.ahead, 2);
    assert_eq!(forward.behind, 2);
    let status_for = |comparison: &BranchFileComparison, path: &str| {
        let (index, item) = comparison
            .files
            .iter()
            .enumerate()
            .find(|(_, f)| f.display_path() == path)
            .unwrap();
        (index, item.status)
    };
    assert_eq!(status_for(&forward, "current-only.txt").1, "D");
    assert_eq!(status_for(&forward, "topic-only.txt").1, "A");
    let modified = status_for(&forward, "shared.txt").0;
    let patch = diff_compared_file(dir.path(), &forward, modified).unwrap();
    assert!(patch.contains("-current\n"), "{patch}");
    assert!(patch.contains("+topic\n"), "{patch}");
    assert!(!patch.contains("uncommitted secret"));
    let snap = open_compared_original_snapshot(dir.path(), &forward, modified).unwrap();
    assert_eq!(snap.content, "current\n");
    assert!(snap.revision_label.unwrap().starts_with("SOURCE"));
    let reverse =
        compare_branch_trees(dir.path(), "topic", CompareDirection::SelectedToCurrent).unwrap();
    assert_eq!(reverse.source_oid, topic_oid.to_string());
    assert_eq!(reverse.target_oid, current_oid.to_string());
    assert_eq!(status_for(&reverse, "current-only.txt").1, "A");
    assert_eq!(status_for(&reverse, "topic-only.txt").1, "D");
    let patch =
        diff_compared_file(dir.path(), &reverse, status_for(&reverse, "shared.txt").0).unwrap();
    assert!(patch.contains("-topic\n"), "{patch}");
    assert!(patch.contains("+current\n"), "{patch}");
    assert_eq!(repo.head().unwrap().target().unwrap(), current_oid);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "uncommitted secret\n"
    );
    let remote = compare_branch_trees(
        dir.path(),
        "refs/remotes/origin/topic",
        CompareDirection::CurrentToSelected,
    )
    .unwrap();
    assert_eq!(remote.target_oid, topic_oid.to_string());
    assert_eq!(remote.files.len(), forward.files.len());
    assert!(remote.title().contains("origin/topic"));
    assert!(compare_branch_files(
        dir.path(),
        "refs/remotes/origin/topic",
        CompareDirection::CurrentToSelected
    )
    .is_err());
}

#[test]
fn rename_preview_is_exact_and_bounded_utf8() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    commit_file(&repo, dir.path(), "old.txt", "same retained text\n", "old");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let current_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    repo.branch("topic", &base, false).unwrap();
    drop(base);
    repo.set_head("refs/heads/topic").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("renamed.txt")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("old.txt")).unwrap();
    index.add_path(Path::new("renamed.txt")).unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = Signature::now("Test", "test@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "rename", &tree, &[&parent])
        .unwrap();
    drop(tree);
    index.write().unwrap();
    repo.set_head(&format!("refs/heads/{current_name}"))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let comparison =
        compare_branch_trees(dir.path(), "topic", CompareDirection::CurrentToSelected).unwrap();
    assert_eq!(comparison.files.len(), 1);
    assert_eq!(comparison.files[0].status, "R");
    let preview = diff_compared_file(dir.path(), &comparison, 0).unwrap();
    assert!(preview.contains("old.txt"), "{preview}");
    assert!(preview.contains("renamed.txt"), "{preview}");
    assert_eq!(
        open_compared_original_snapshot(dir.path(), &comparison, 0)
            .unwrap()
            .content,
        "same retained text\n"
    );
    let mut stale = comparison.clone();
    stale.files[0].old_path = Some(PathBuf::from("../unsafe"));
    assert!(diff_compared_file(dir.path(), &stale, 0).is_err());
    stale = comparison.clone();
    stale.files[0].status = "M";
    assert!(diff_compared_file(dir.path(), &stale, 0).is_err());
}
#[test]
fn binary_and_large_multibyte_patch_previews_remain_bounded() {
    assert!(bounded_text(b"binary\0object").unwrap().contains("Binary"));
    assert!(bounded_text(&vec![b'x'; MAX_PREVIEW_BYTES + 1])
        .unwrap()
        .contains("preview unavailable"));
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", false)
        .unwrap();
    commit_file(&repo, dir.path(), "base.txt", "baseline\n", "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let current_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    repo.branch("topic", &base, false).unwrap();
    drop(base);
    repo.set_head("refs/heads/topic").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let unicode_line = format!("{}\n", "界".repeat(25000));
    commit_file(&repo, dir.path(), "large.txt", &unicode_line, "large");
    repo.set_head(&format!("refs/heads/{current_name}"))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let comparison =
        compare_branch_trees(dir.path(), "topic", CompareDirection::CurrentToSelected).unwrap();
    let index = comparison
        .files
        .iter()
        .position(|f| f.display_path() == "large.txt")
        .unwrap();
    let text = diff_compared_file(dir.path(), &comparison, index).unwrap();
    assert!(text.len() <= MAX_PREVIEW_BYTES + 96);
    assert!(text.contains("large.txt"));
    assert!(!text.contains("uncommitted secret"));
    assert!(diff_compared_file(dir.path(), &comparison, index + 10).is_err());
}

#[test]
fn invalid_names_paths_and_stale_indices_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    commit_file(&repo, dir.path(), "file.txt", "base\n", "initial");
    for name in ["", "topic~1", "../topic", "missing"] {
        assert!(
            compare_branch_files(dir.path(), name, CompareDirection::CurrentToSelected).is_err(),
            "{name}"
        );
    }
    let snapshot = compare_branch_files(
        dir.path(),
        repo.head().unwrap().shorthand().unwrap(),
        CompareDirection::CurrentToSelected,
    )
    .unwrap();
    assert!(diff_compared_file(dir.path(), &snapshot, 0).is_err());
}
