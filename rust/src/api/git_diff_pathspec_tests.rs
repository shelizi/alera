use super::*;

#[test]
#[cfg(unix)]
fn git_diff_treats_star_pathspec_characters_as_literals() {
    let repo = init_repo();
    std::fs::write(repo.path().join("*.txt"), "literal star\n").expect("write star file");
    std::fs::write(repo.path().join("other.txt"), "other file\n").expect("write other file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add pathspec files"]);

    std::fs::write(repo.path().join("*.txt"), "literal star changed\n").expect("modify star");
    std::fs::write(repo.path().join("other.txt"), "other changed\n").expect("modify other");

    let diff = git_diff(
        path_str(repo.path()),
        "*.txt".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "*.txt");
    assert!(diff_text(&diff.files[0]).contains("literal star changed"));
    assert!(!diff_text(&diff.files[0]).contains("other changed"));
}

#[test]
fn git_diff_treats_bracket_pathspec_characters_as_literals() {
    let repo = init_repo();
    std::fs::write(repo.path().join("file[1].txt"), "literal bracket\n")
        .expect("write bracket file");
    std::fs::write(repo.path().join("file1.txt"), "matched by glob\n").expect("write glob file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add bracket files"]);

    std::fs::write(repo.path().join("file[1].txt"), "literal bracket changed\n")
        .expect("modify bracket");
    std::fs::write(repo.path().join("file1.txt"), "glob changed\n").expect("modify glob");

    let diff = git_diff(
        path_str(repo.path()),
        "file[1].txt".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "file[1].txt");
    assert!(diff_text(&diff.files[0]).contains("literal bracket changed"));
    assert!(!diff_text(&diff.files[0]).contains("glob changed"));
}
