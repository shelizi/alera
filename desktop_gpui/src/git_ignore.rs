//! Guarded Source Control "Add to .gitignore" for an untracked file.
//! Append an exact anchored rule; never overwrite an existing ignore file,
//! follow its symlink, or turn a filename into a Git wildcard pattern.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path};

const MAX_IGNORE_BYTES: usize = 2 * 1024 * 1024;

fn exact_ignore_rule(relative: &Path) -> Result<String, String> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Unsafe Git ignore path".into());
    }
    let mut segments = Vec::new();
    for component in relative.components() {
        let segment = component
            .as_os_str()
            .to_str()
            .ok_or("Git ignore path must be Unicode")?;
        if segment.chars().any(|ch| ch == '\n' || ch == '\r') || segment == ".git" {
            return Err("Unsafe Git ignore filename".into());
        }
        let mut escaped = String::new();
        for ch in segment.chars() {
            if matches!(ch, '*' | '?' | '[' | ']' | '\\' | '!' | '#') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        segments.push(escaped);
    }
    let rule = format!("/{}", segments.join("/"));
    if rule == "/.gitignore" {
        return Err("Cannot ignore the repository .gitignore file".into());
    }
    Ok(rule)
}

pub fn add_untracked_file_to_gitignore(workspace: &Path, relative: &Path) -> Result<bool, String> {
    let rule = exact_ignore_rule(relative)?;
    let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
    let root = repo.workdir().ok_or("No Git working tree")?;
    let target = root.join(relative);
    let file_type = fs::symlink_metadata(&target)
        .map_err(|e| format!("Untracked file no longer exists: {e}"))?
        .file_type();
    if !file_type.is_file() && !file_type.is_symlink() {
        return Err("Select a regular untracked file".into());
    }

    let ignore_path = root.join(".gitignore");
    let existing = match fs::symlink_metadata(&ignore_path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.len() > MAX_IGNORE_BYTES as u64 {
                return Err("Existing .gitignore is not a safe, bounded regular file".into());
            }
            let bytes = fs::read(&ignore_path).map_err(|e| e.to_string())?;
            String::from_utf8(bytes).map_err(|_| ".gitignore is not valid UTF-8".to_string())?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    if existing
        .lines()
        .any(|line| line.trim_end_matches('\r') == rule)
    {
        return Ok(false);
    }
    let status = repo.status_file(relative).map_err(|e| e.to_string())?;
    if !status.contains(git2::Status::WT_NEW) || status.contains(git2::Status::CONFLICTED) {
        return Err("File is no longer untracked; refresh Source Control".into());
    }
    let newline = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let appended = format!("{newline}{rule}\n");
    if existing.len().saturating_add(appended.len()) > MAX_IGNORE_BYTES {
        return Err(".gitignore is too large to append safely".into());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ignore_path)
        .map_err(|e| e.to_string())?;
    file.write_all(appended.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_rule_is_exact_and_escaped() {
        assert_eq!(
            exact_ignore_rule(Path::new("assets/a[1] *!.txt")).unwrap(),
            "/assets/a\\[1\\] \\*\\!.txt"
        );
        for unsafe_path in [
            "",
            ".",
            "../oops",
            "docs/../oops",
            "/outside",
            ".git/config",
            "bad\nname",
        ] {
            assert!(
                exact_ignore_rule(Path::new(unsafe_path)).is_err(),
                "{unsafe_path}"
            );
        }
    }

    #[test]
    fn gitignore_append_is_idempotent_and_preserves_existing_content() {
        let tmp = tempfile::tempdir().unwrap();
        let _repo = git2::Repository::init(tmp.path()).unwrap();
        fs::create_dir(tmp.path().join("assets")).unwrap();
        fs::write(tmp.path().join("assets").join("a[1].txt"), "untracked").unwrap();
        fs::write(tmp.path().join(".gitignore"), "existing-rule\r\n").unwrap();
        let target = Path::new("assets/a[1].txt");
        assert!(add_untracked_file_to_gitignore(tmp.path(), target).unwrap());
        assert_eq!(
            fs::read_to_string(tmp.path().join(".gitignore")).unwrap(),
            "existing-rule\r\n/assets/a\\[1\\].txt\n"
        );
        assert!(!add_untracked_file_to_gitignore(tmp.path(), target).unwrap_or(false));
    }

    #[test]
    fn tracked_and_missing_paths_never_modify_gitignore() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("tracked.txt"), "tracked").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        assert!(add_untracked_file_to_gitignore(tmp.path(), Path::new("tracked.txt")).is_err());
        assert!(add_untracked_file_to_gitignore(tmp.path(), Path::new("missing.txt")).is_err());
        assert!(!tmp.path().join(".gitignore").exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_gitignore_without_touching_external_file() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let _repo = git2::Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("new.txt"), "new").unwrap();
        fs::write(outside.path().join("target.txt"), "untouched").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("target.txt"),
            tmp.path().join(".gitignore"),
        )
        .unwrap();
        assert!(add_untracked_file_to_gitignore(tmp.path(), Path::new("new.txt")).is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("target.txt")).unwrap(),
            "untouched"
        );
    }
}
