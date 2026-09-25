use super::*;
use git2::{Oid, Repository};
use std::path::Path;
use std::process::Command;

#[path = "git_ancestry_tests.rs"]
mod git_ancestry_tests;
#[path = "git_clone_tests.rs"]
mod git_clone_tests;
#[path = "git_commit_tests.rs"]
mod git_commit_tests;
#[path = "git_diff_blob_tests.rs"]
mod git_diff_blob_tests;
#[path = "git_diff_edge_tests.rs"]
mod git_diff_edge_tests;
#[path = "git_diff_pathspec_tests.rs"]
mod git_diff_pathspec_tests;
#[path = "git_diff_tests.rs"]
mod git_diff_tests;
#[path = "git_discard_tests.rs"]
mod git_discard_tests;
#[path = "git_explorer_status_tests.rs"]
mod git_explorer_status_tests;
#[path = "git_history_tests.rs"]
mod git_history_tests;
#[path = "git_range_tests.rs"]
mod git_range_tests;
#[path = "git_refresh_source_tests.rs"]
mod git_refresh_source_tests;
#[path = "git_stage_pathspec_tests.rs"]
mod git_stage_pathspec_tests;
#[path = "git_stage_tests.rs"]
mod git_stage_tests;
#[path = "git_stash_tests.rs"]
mod git_stash_tests;
#[path = "git_status_tests.rs"]
mod git_status_tests;
#[path = "git_submodule_tests.rs"]
mod git_submodule_tests;
#[path = "git_sync_tests.rs"]
mod git_sync_tests;
#[path = "git_worktree_tests.rs"]
mod git_worktree_tests;

fn run_git(dir: &Path, args: &[&str]) {
    let status = git_command(dir, args).status().expect("git command runs");
    assert!(status.success(), "git {:?} failed", args);
}

fn run_git_expect_failure(dir: &Path, args: &[&str]) {
    let status = git_command(dir, args).status().expect("git command runs");
    assert!(!status.success(), "git {:?} succeeded unexpectedly", args);
}

// Test fixture: it runs from a console, so the console-window suppression in
// `alera_core::child_process` does not apply.
#[allow(clippy::disallowed_methods)]
fn git_command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(["-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");
    command
}

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "-b", "main"]);
    configure_git_identity(dir.path());
    std::fs::write(dir.path().join("README.md"), "hello").expect("write");
    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-m", "initial"]);
    dir
}

fn configure_git_identity(dir: &Path) {
    run_git(dir, &["config", "user.name", "Test"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "core.autocrlf", "false"]);
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn canonical(path: &str) -> String {
    let target = Path::new(path);
    dunce::canonicalize(target)
        .unwrap_or_else(|_| target.to_path_buf())
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string()
}

fn branch_oid(path: &Path, branch: &str) -> Oid {
    let repo = Repository::open(path).expect("open repo");
    let branch = repo
        .find_branch(branch, git2::BranchType::Local)
        .expect("find branch");
    branch.get().target().expect("branch target")
}

fn commit_file(repo_path: &Path, file_name: &str, content: &str, message: &str) {
    std::fs::write(repo_path.join(file_name), content).expect("write file");
    run_git(repo_path, &["add", file_name]);
    run_git(repo_path, &["commit", "-m", message]);
}

fn head_commit_message(path: &Path) -> String {
    let repo = Repository::open(path).expect("open repo");
    let commit = repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("head commit");
    commit.message().expect("message").to_string()
}

fn head_author_name(path: &Path) -> String {
    let repo = Repository::open(path).expect("open repo");
    let commit = repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("head commit");
    let author = commit.author().name().expect("author name").to_string();
    author
}

fn head_committer_name(path: &Path) -> String {
    let repo = Repository::open(path).expect("open repo");
    let commit = repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("head commit");
    let committer = commit
        .committer()
        .name()
        .expect("committer name")
        .to_string();
    committer
}

fn diff_text(file: &GitDiffFile) -> String {
    file.lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn rejects_bare_repository() {
    let bare = tempfile::tempdir().expect("tempdir");
    run_git(bare.path(), &["init", "--bare"]);

    assert!(!is_git_repository(path_str(bare.path())).unwrap());
}

#[test]
fn reports_branches_and_current() {
    let repo = init_repo();
    run_git(repo.path(), &["branch", "feature"]);

    let branches = list_branches(path_str(repo.path())).unwrap();
    assert!(branches.contains(&"main".to_string()));
    assert!(branches.contains(&"feature".to_string()));

    assert_eq!(current_branch(path_str(repo.path())).unwrap(), "main");
    assert!(branch_exists(path_str(repo.path()), "feature".to_string()).unwrap());
    assert!(!branch_exists(path_str(repo.path()), "missing".to_string()).unwrap());
}

#[test]
fn validates_branch_names() {
    assert!(is_valid_branch_name("feature/login".to_string()).unwrap());
    assert!(!is_valid_branch_name("bad branch".to_string()).unwrap());
}

#[test]
fn repository_state_includes_head_message() {
    let repo = init_repo();

    let state = git_repository_state(path_str(repo.path())).unwrap();

    assert_eq!(state.head_message.as_deref(), Some("initial"));
}
