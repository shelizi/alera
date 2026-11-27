use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;

use super::{
    entry_for_path_filtered, is_protected_child_path, resolve_existing, workspace_root,
    WorkspaceFileEntry, WorkspaceFileError, WorkspaceFileErrorKind, WorkspaceFileKind,
};

const LISTABLE_DESCENDANT_MAX_DEPTH: usize = 32;
const LISTABLE_DESCENDANT_MAX_VISITS: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HideIgnoredDirectoryKind {
    Listable,
    Empty,
    IgnoredOnly,
    Unknown,
}

#[cfg(test)]
pub(super) fn list_workspace_children(
    workspace_path: String,
    relative_path: String,
    hide_ignored: bool,
) -> Result<Vec<WorkspaceFileEntry>, WorkspaceFileError> {
    list_workspace_children_filtered(workspace_path, relative_path, hide_ignored, false)
}

pub(super) fn list_workspace_children_filtered(
    workspace_path: String,
    relative_path: String,
    hide_ignored: bool,
    hide_hidden: bool,
) -> Result<Vec<WorkspaceFileEntry>, WorkspaceFileError> {
    list_workspace_children_with_policy(
        workspace_path,
        relative_path,
        hide_ignored,
        hide_hidden,
        true,
    )
}

fn list_workspace_children_with_policy(
    workspace_path: String,
    relative_path: String,
    hide_ignored: bool,
    hide_hidden: bool,
    reuse_enumerated_metadata: bool,
) -> Result<Vec<WorkspaceFileEntry>, WorkspaceFileError> {
    let root = workspace_root(&workspace_path)?;
    let directory = resolve_existing(&root, &relative_path)?;
    let metadata = fs::symlink_metadata(&directory)
        .map_err(|error| WorkspaceFileError::from_io(error, &relative_path))?;
    if !metadata.is_dir() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            relative_path,
        ));
    }

    let children = if hide_ignored {
        ignored_aware_children(&directory, reuse_enumerated_metadata)?
    } else {
        read_dir_children(&directory, reuse_enumerated_metadata)?
    };

    let mut entries = Vec::with_capacity(children.len());
    for child in children {
        // One unreadable child must not blank the whole directory. Preserve
        // path-based stat for ambiguous names and non-regular entries.
        let (path, metadata) = child.into_parts();
        if let Ok(Some(entry)) =
            entry_for_path_filtered(&root, &path, hide_ignored, hide_hidden, metadata)
        {
            entries.push(entry);
        }
    }
    // The alternate policy is only used for parity and performance tests.
    #[cfg(test)]
    if !reuse_enumerated_metadata {
        sort_entries_uncached(&mut entries);
        return Ok(entries);
    }
    sort_entries(&mut entries);
    Ok(entries)
}

fn sort_entries(entries: &mut [WorkspaceFileEntry]) {
    // Compute Unicode lowercase keys once per row instead of allocating two
    // temporary strings in every comparison. Preserve exact tie ordering.
    entries.sort_by_cached_key(|entry| {
        (
            !matches!(entry.kind, WorkspaceFileKind::Directory),
            entry.name.to_lowercase(),
            entry.name.clone(),
        )
    });
}

#[cfg(test)]
fn sort_entries_uncached(entries: &mut [WorkspaceFileEntry]) {
    entries.sort_by(|left, right| {
        let left_dir = matches!(left.kind, WorkspaceFileKind::Directory);
        let right_dir = matches!(right.kind, WorkspaceFileKind::Directory);
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
}

struct ListingChild {
    path: PathBuf,
    #[cfg(windows)]
    metadata: Option<fs::Metadata>,
}

impl ListingChild {
    fn new(path: PathBuf, reuse: bool, metadata: impl FnOnce() -> Option<fs::Metadata>) -> Self {
        #[cfg(windows)]
        let metadata = if reuse && is_ordinary_windows_filename(&path) {
            metadata().filter(reusable_windows_metadata)
        } else {
            None
        };
        #[cfg(not(windows))]
        let _ = (reuse, metadata);
        Self {
            path,
            #[cfg(windows)]
            metadata,
        }
    }

    fn into_parts(self) -> (PathBuf, Option<fs::Metadata>) {
        #[cfg(windows)]
        {
            (self.path, self.metadata)
        }
        #[cfg(not(windows))]
        {
            (self.path, None)
        }
    }
}

// Windows directory enumeration already carries file size, timestamps and
// attributes. Limit reuse to ordinary ASCII names; preserve the old path
// stat for device aliases, unusual Win32 names, Unicode, directories,
// symlinks and reparse points. This is listing metadata only: all actual
// reads/writes/mutations still resolve paths and obtain fresh metadata.
#[cfg(windows)]
fn is_ordinary_windows_filename(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name.is_empty()
        || name.ends_with('.')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or_default();
    if ["CON", "PRN", "AUX", "NUL", "CLOCK"]
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
    {
        return false;
    }
    let bytes = stem.as_bytes();
    if bytes.len() == 4
        && (bytes[..3].eq_ignore_ascii_case(b"COM") || bytes[..3].eq_ignore_ascii_case(b"LPT"))
        && bytes[3].is_ascii_digit()
    {
        return false;
    }
    true
}

#[cfg(windows)]
fn reusable_windows_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

fn ignored_aware_children(
    directory: &Path,
    reuse: bool,
) -> Result<Vec<ListingChild>, WorkspaceFileError> {
    let mut paths = Vec::new();
    let walker = WalkBuilder::new(directory)
        .max_depth(Some(1))
        .hidden(false)
        .parents(true)
        .require_git(false)
        .build();
    for result in walker {
        let entry = result.map_err(|error| {
            WorkspaceFileError::new(WorkspaceFileErrorKind::Io, error.to_string())
        })?;
        if entry.path() == directory {
            continue;
        }
        if is_protected_child_path(entry.path()) {
            continue;
        }
        paths.push(ListingChild::new(entry.path().to_path_buf(), reuse, || {
            entry.metadata().ok()
        }));
    }
    Ok(paths)
}

fn read_dir_children(
    directory: &Path,
    reuse: bool,
) -> Result<Vec<ListingChild>, WorkspaceFileError> {
    let mut paths = Vec::new();
    for result in fs::read_dir(directory)
        .map_err(|error| WorkspaceFileError::from_io(error, directory.to_string_lossy()))?
    {
        let entry = result
            .map_err(|error| WorkspaceFileError::from_io(error, directory.to_string_lossy()))?;
        let path = entry.path();
        if is_protected_child_path(&path) {
            continue;
        }
        paths.push(ListingChild::new(path, reuse, || entry.metadata().ok()));
    }
    Ok(paths)
}

pub(super) fn has_unprotected_child(directory: &Path) -> Result<bool, WorkspaceFileError> {
    for result in fs::read_dir(directory)
        .map_err(|error| WorkspaceFileError::from_io(error, directory.to_string_lossy()))?
    {
        let entry = result
            .map_err(|error| WorkspaceFileError::from_io(error, directory.to_string_lossy()))?;
        if !is_protected_child_path(&entry.path()) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn hide_ignored_directory_kind(directory: &Path) -> HideIgnoredDirectoryKind {
    match has_unprotected_child(directory) {
        Ok(false) => HideIgnoredDirectoryKind::Empty,
        Ok(true) => walk_listable_descendant(directory),
        Err(_) => HideIgnoredDirectoryKind::Unknown,
    }
}

fn lock_visited_directories(
    visited: &Mutex<HashSet<(u64, u64)>>,
) -> std::sync::MutexGuard<'_, HashSet<(u64, u64)>> {
    visited
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn walk_listable_descendant(directory: &Path) -> HideIgnoredDirectoryKind {
    walk_listable_descendant_with_probe(directory, has_unprotected_child)
}

fn walk_listable_descendant_with_probe(
    directory: &Path,
    mut has_child: impl FnMut(&Path) -> Result<bool, WorkspaceFileError>,
) -> HideIgnoredDirectoryKind {
    let visited = Arc::new(Mutex::new(HashSet::new()));
    if let Some(id) = directory_identity(directory) {
        lock_visited_directories(&visited).insert(id);
    }
    let visited_for_filter = visited;
    let walker = WalkBuilder::new(directory)
        .hidden(false)
        .parents(true)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(LISTABLE_DESCENDANT_MAX_DEPTH))
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            if is_protected_child_path(entry.path()) {
                return false;
            }
            if entry
                .file_type()
                .is_some_and(|file_type| file_type.is_dir())
            {
                if let Some(id) = directory_identity(entry.path()) {
                    return lock_visited_directories(&visited_for_filter).insert(id);
                }
            }
            true
        })
        .build();

    let mut truncated = false;
    let mut saw_error = false;
    let mut visits = 0;
    // The depth-first walker already proves that an ancestor contains an
    // unprotected child whenever it yields a descendant. Only probe a
    // directory with no yielded child to distinguish physically empty from
    // ignored-only. This avoids reopening every intermediate directory.
    let mut pending_directory: Option<PathBuf> = None;
    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => {
                saw_error = true;
                continue;
            }
        };
        if entry.path() == directory {
            continue;
        }
        if let Some(pending) = pending_directory.take() {
            if !entry.path().starts_with(&pending) && !matches!(has_child(&pending), Ok(true)) {
                return HideIgnoredDirectoryKind::Listable;
            }
        }
        visits += 1;
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            return HideIgnoredDirectoryKind::Listable;
        }
        if entry.depth() >= LISTABLE_DESCENDANT_MAX_DEPTH
            || visits >= LISTABLE_DESCENDANT_MAX_VISITS
        {
            // No later yielded child can prove non-emptiness at the limit.
            // Preserve the original empty/unreadable directory behavior.
            if !matches!(has_child(entry.path()), Ok(true)) {
                return HideIgnoredDirectoryKind::Listable;
            }
            truncated = true;
        } else {
            pending_directory = Some(entry.path().to_path_buf());
        }
        if visits >= LISTABLE_DESCENDANT_MAX_VISITS {
            break;
        }
    }
    if let Some(pending) = pending_directory {
        if !matches!(has_child(&pending), Ok(true)) {
            return HideIgnoredDirectoryKind::Listable;
        }
    }
    if truncated || saw_error {
        HideIgnoredDirectoryKind::Unknown
    } else {
        HideIgnoredDirectoryKind::IgnoredOnly
    }
}

fn directory_identity(path: &Path) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path).ok()?;
        metadata
            .file_type()
            .is_dir()
            .then(|| (metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::path::PathBuf;

    fn workspace_path(dir: &tempfile::TempDir) -> String {
        dir.path().to_string_lossy().to_string()
    }

    fn entry_names(entries: &[WorkspaceFileEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[cfg(unix)]
    struct RestoreUnixMode {
        path: PathBuf,
        mode: u32,
    }

    #[cfg(unix)]
    impl Drop for RestoreUnixMode {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
        }
    }

    fn list_with_policy(
        workspace: &tempfile::TempDir,
        relative: &str,
        hide_ignored: bool,
        hide_hidden: bool,
        optimized: bool,
    ) -> Vec<WorkspaceFileEntry> {
        list_workspace_children_with_policy(
            workspace_path(workspace),
            relative.to_string(),
            hide_ignored,
            hide_hidden,
            optimized,
        )
        .unwrap()
    }

    #[test]
    fn optimized_listing_matches_fresh_path_stats_and_ignore_modes() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        git2::Repository::init(root).unwrap();
        fs::write(root.join(".gitignore"), "*.tmp\nonly-ignored/*\n").unwrap();
        fs::write(root.join(".ignore"), "ignored-by-ignore.txt\n").unwrap();
        fs::write(root.join(".git/info/exclude"), "local-only.txt\n").unwrap();
        for name in [
            "z.txt",
            "Alpha.txt",
            "beta.txt",
            "case-file.RS",
            "space name.txt",
            "unicode-\u{00e9}.txt",
            ".hidden",
            "ignored.tmp",
            "ignored-by-ignore.txt",
            "local-only.txt",
        ] {
            fs::write(root.join(name), name).unwrap();
        }
        for name in ["src", "empty", "only-ignored", "nested/ignored"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("src/.gitignore"), "nested.tmp\n").unwrap();
        fs::write(root.join("src/nested.tmp"), "ignored").unwrap();
        fs::write(root.join("only-ignored/data.txt"), "ignored").unwrap();
        fs::write(root.join("nested/ignored/data.tmp"), "ignored").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("src/main.rs"), root.join("link.rs")).unwrap();

        for relative in ["", "src", "nested"] {
            for hide_ignored in [false, true] {
                for hide_hidden in [false, true] {
                    assert_eq!(
                        list_with_policy(&workspace, relative, hide_ignored, hide_hidden, false),
                        list_with_policy(&workspace, relative, hide_ignored, hide_hidden, true),
                        "listing mismatch: {relative}, ignored={hide_ignored}, hidden={hide_hidden}",
                    );
                }
            }
        }
        let before = list_with_policy(&workspace, "", true, false, true);
        assert!(!entry_names(&before).contains(&"ignored.tmp"));
        assert!(!entry_names(&before).contains(&"local-only.txt"));
        assert!(!entry_names(&before).contains(&"ignored-by-ignore.txt"));
        assert!(!entry_names(&before).contains(&"only-ignored"));
        assert!(entry_names(&before).contains(&"empty"));
        let old_token = before
            .iter()
            .find(|entry| entry.name == "z.txt")
            .unwrap()
            .content_token
            .clone();
        fs::write(root.join("z.txt"), "fresh larger content").unwrap();
        fs::write(root.join(".gitignore"), "only-ignored/*\n").unwrap();
        fs::remove_file(root.join("Alpha.txt")).unwrap();
        let after = list_with_policy(&workspace, "", true, false, true);
        assert_eq!(after, list_with_policy(&workspace, "", true, false, false));
        assert!(entry_names(&after).contains(&"ignored.tmp"));
        assert!(!entry_names(&after).contains(&"Alpha.txt"));
        assert_ne!(
            after
                .iter()
                .find(|entry| entry.name == "z.txt")
                .unwrap()
                .content_token,
            old_token,
        );
    }

    #[cfg(windows)]
    #[test]
    fn cached_windows_metadata_only_used_for_safe_regular_files() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::write(root.join("ordinary.txt"), "ordinary").unwrap();
        fs::write(root.join("space name.txt"), "space").unwrap();
        fs::write(root.join("unicode-\u{00e9}.txt"), "unicode").unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        for name in [
            "nul",
            "NUL.txt",
            "aux",
            "CON.txt",
            "PRN",
            "COM1.rs",
            "lpt9.txt",
            "COM0",
            "LPT0",
            "COM\u{00b9}.txt",
            "file.",
            "file ",
            "CONIN$",
            "CONOUT$",
            "CLOCK$",
            "file:name",
            "space name.txt",
            "unicode-\u{00e9}.txt",
        ] {
            assert!(!is_ordinary_windows_filename(Path::new(name)), "{name}");
        }
        for name in ["ordinary.txt", "Main_RS-1.rs", ".gitignore", "COM10.txt"] {
            assert!(is_ordinary_windows_filename(Path::new(name)), "{name}");
        }
        for children in [
            read_dir_children(root, true).unwrap(),
            ignored_aware_children(root, true).unwrap(),
        ] {
            let cached: Vec<_> = children
                .into_iter()
                .filter_map(|child| {
                    let (path, metadata) = child.into_parts();
                    metadata.map(|metadata| {
                        assert_eq!(metadata.len(), fs::symlink_metadata(&path).unwrap().len());
                        path.file_name().unwrap().to_string_lossy().into_owned()
                    })
                })
                .collect();
            assert_eq!(cached, vec!["ordinary.txt"]);
        }
        assert!(read_dir_children(root, false)
            .unwrap()
            .into_iter()
            .all(|child| child.into_parts().1.is_none()));
    }

    #[test]
    #[ignore = "filesystem performance benchmark"]
    fn benchmark_workspace_listing_enumerated_metadata() {
        use std::time::{Duration, Instant};
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        git2::Repository::init(root).unwrap();
        fs::write(root.join(".gitignore"), "*.tmp\n").unwrap();
        for index in 0..1600 {
            let name = if index % 5 == 0 {
                format!("cache-{index:05}.tmp")
            } else if index % 2 == 0 {
                format!("Item-{index:05}.rs")
            } else {
                format!("item-{index:05}.txt")
            };
            fs::write(root.join(name), "fixture\n").unwrap();
        }
        for index in 0..16 {
            let dir = root.join(format!("dir-{index:02}"));
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("nested.txt"), "nested").unwrap();
        }
        fn median(values: &mut [Duration]) -> f64 {
            values.sort();
            values[values.len() / 2].as_secs_f64() * 1000.0
        }
        for hide_ignored in [false, true] {
            let expected = list_with_policy(&workspace, "", hide_ignored, false, false);
            assert_eq!(
                expected,
                list_with_policy(&workspace, "", hide_ignored, false, true)
            );
            let mut old = Vec::new();
            let mut new = Vec::new();
            for index in 0..7 {
                for optimized in if index % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let started = Instant::now();
                    let entries = list_with_policy(&workspace, "", hide_ignored, false, optimized);
                    let elapsed = started.elapsed();
                    assert_eq!(entries, expected);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("listing hide_ignored={hide_ignored} rows={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", expected.len(), old / new);
        }
    }

    #[test]
    fn listable_walk_skips_reopening_intermediate_directories() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let root = workspace.path().join("source");
        let mut leaf = root.clone();
        for index in 0..20 {
            leaf.push(format!("d{index}"));
        }
        fs::create_dir_all(&leaf).expect("create nested directories");
        fs::write(leaf.join("visible.txt"), "visible").expect("write visible file");
        let mut probes = Vec::new();
        let result = walk_listable_descendant_with_probe(&root, |path| {
            probes.push(path.to_path_buf());
            has_unprotected_child(path)
        });
        assert_eq!(result, HideIgnoredDirectoryKind::Listable);
        assert!(probes.is_empty(), "ancestors already have visible children");

        fs::remove_file(leaf.join("visible.txt")).expect("empty leaf");
        let result = walk_listable_descendant_with_probe(&root, |path| {
            probes.push(path.to_path_buf());
            has_unprotected_child(path)
        });
        assert_eq!(result, HideIgnoredDirectoryKind::Listable);
        assert_eq!(probes, vec![leaf.clone()]);

        fs::write(workspace.path().join(".gitignore"), "*.jsonl\n").expect("ignore sessions");
        fs::write(leaf.join("session.jsonl"), "session").expect("write ignored leaf");
        probes.clear();
        let result = walk_listable_descendant_with_probe(&root, |path| {
            probes.push(path.to_path_buf());
            has_unprotected_child(path)
        });
        assert_eq!(result, HideIgnoredDirectoryKind::IgnoredOnly);
        assert_eq!(probes, vec![leaf]);
    }

    #[test]
    fn list_workspace_children_sorts_directories_and_filters_ignored_entries() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        fs::create_dir(workspace.path().join("src")).expect("create src");
        fs::write(workspace.path().join("src/main.dart"), "void main() {}\n").expect("write src");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");
        fs::write(workspace.path().join("ignored.txt"), "ignored\n").expect("write ignored");

        let entries =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let names = entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"src"));
        assert!(names.contains(&"readme.md"));
        assert!(!names.contains(&"ignored.txt"));
        assert!(
            names.iter().position(|name| *name == "src").unwrap()
                < names.iter().position(|name| *name == "readme.md").unwrap()
        );

        let src = entries.iter().find(|entry| entry.name == "src").unwrap();
        assert_eq!(src.kind, WorkspaceFileKind::Directory);
        assert!(src.has_children_hint);
    }

    #[test]
    fn list_workspace_children_omits_directories_that_only_contain_ignored_files() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(".gitignore"), ".codex/*\n").expect("write gitignore");
        fs::create_dir(workspace.path().join(".codex")).expect("create .codex");
        fs::write(workspace.path().join(".codex/config.json"), "{}\n").expect("write ignored file");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        let hidden =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let hidden_names = entry_names(&hidden);
        assert!(hidden_names.contains(&"readme.md"));
        assert!(!hidden_names.contains(&".codex"));

        let shown =
            list_workspace_children(workspace_path(&workspace), String::new(), false).unwrap();
        let shown_names = entry_names(&shown);
        assert!(shown_names.contains(&".codex"));
        assert!(shown_names.contains(&"readme.md"));
    }

    #[test]
    fn list_workspace_children_omits_nested_ignored_only_trees() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(".gitignore"), "*.jsonl\n").expect("write gitignore");
        fs::create_dir_all(workspace.path().join(".codex/sessions")).expect("create sessions");
        fs::write(
            workspace.path().join(".codex/sessions/a.jsonl"),
            "session\n",
        )
        .expect("write session");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        let hidden =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let hidden_names = entry_names(&hidden);
        assert!(hidden_names.contains(&"readme.md"));
        assert!(!hidden_names.contains(&".codex"));

        let shown =
            list_workspace_children(workspace_path(&workspace), String::new(), false).unwrap();
        assert!(entry_names(&shown).contains(&".codex"));
    }

    #[test]
    fn list_workspace_children_keeps_directories_with_tracked_and_ignored_files() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        fs::create_dir(workspace.path().join("src")).expect("create src");
        fs::write(workspace.path().join("src/ignored.txt"), "ignored\n").expect("write ignored");
        fs::write(workspace.path().join("src/main.dart"), "void main() {}\n").expect("write src");

        let root =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let src = root.iter().find(|entry| entry.name == "src").unwrap();
        assert_eq!(src.kind, WorkspaceFileKind::Directory);
        assert!(src.has_children_hint);

        let src_children =
            list_workspace_children(workspace_path(&workspace), "src".to_string(), true).unwrap();
        let src_names = entry_names(&src_children);
        assert!(src_names.contains(&"main.dart"));
        assert!(!src_names.contains(&"ignored.txt"));
    }

    #[test]
    fn list_workspace_children_keeps_truly_empty_directories_when_hiding_ignored() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::create_dir(workspace.path().join("empty")).expect("create empty");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        let entries =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let empty = entries.iter().find(|entry| entry.name == "empty").unwrap();
        assert_eq!(empty.kind, WorkspaceFileKind::Directory);
        assert!(!empty.has_children_hint);
        assert!(entry_names(&entries).contains(&"readme.md"));
    }

    #[test]
    fn list_workspace_children_keeps_directories_of_empty_subdirectories_when_hiding_ignored() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(workspace.path().join("parent/child")).expect("create child");

        let root =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        assert_eq!(entry_names(&root), vec!["parent"]);
        assert!(root[0].has_children_hint);

        let parent =
            list_workspace_children(workspace_path(&workspace), "parent".to_string(), true)
                .unwrap();
        assert_eq!(entry_names(&parent), vec!["child"]);
        assert!(!parent[0].has_children_hint);
    }

    #[cfg(unix)]
    #[test]
    fn list_workspace_children_keeps_directory_when_descendant_probe_fails() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = tempfile::tempdir().expect("tempdir");
        let secret = workspace.path().join("foo/secret");
        fs::create_dir_all(&secret).expect("create secret");
        fs::write(secret.join("hidden.txt"), "hidden\n").expect("write hidden");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).expect("deny secret");
        let _restore = RestoreUnixMode {
            path: secret.clone(),
            mode: 0o700,
        };
        if fs::read_dir(&secret).is_ok() {
            return;
        }

        let hidden =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let hidden_names = entry_names(&hidden);
        assert!(hidden_names.contains(&"foo"));
        assert!(hidden_names.contains(&"readme.md"));
        let foo = hidden.iter().find(|entry| entry.name == "foo").unwrap();
        assert!(foo.has_children_hint);
    }

    #[cfg(unix)]
    #[test]
    fn list_workspace_children_keeps_expander_when_unreadable_dir_has_visible_sibling() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = tempfile::tempdir().expect("tempdir");
        let secret = workspace.path().join("foo/secret");
        fs::create_dir_all(&secret).expect("create secret");
        fs::write(secret.join("hidden.txt"), "hidden\n").expect("write hidden");
        fs::write(workspace.path().join("foo/visible.txt"), "visible\n").expect("write visible");
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).expect("deny secret");
        let _restore = RestoreUnixMode {
            path: secret.clone(),
            mode: 0o700,
        };
        if fs::read_dir(&secret).is_ok() {
            return;
        }

        let hidden =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let foo = hidden.iter().find(|entry| entry.name == "foo").unwrap();
        assert!(foo.has_children_hint);

        let foo_children =
            list_workspace_children(workspace_path(&workspace), "foo".to_string(), true).unwrap();
        let foo_names = entry_names(&foo_children);
        assert!(foo_names.contains(&"visible.txt"));
        assert!(foo_names.contains(&"secret"));
    }

    #[test]
    fn list_workspace_children_keeps_directory_when_listable_walk_hits_depth_bound() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        let mut nested = workspace.path().join("deep");
        for index in 0..=LISTABLE_DESCENDANT_MAX_DEPTH {
            nested.push(format!("d{index}"));
        }
        fs::create_dir_all(&nested).expect("create deep tree");
        fs::write(nested.join("ignored.txt"), "ignored\n").expect("write ignored");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        let hidden =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let hidden_names = entry_names(&hidden);
        assert!(hidden_names.contains(&"deep"));
        assert!(hidden_names.contains(&"readme.md"));
        let deep = hidden.iter().find(|entry| entry.name == "deep").unwrap();
        assert!(deep.has_children_hint);
    }

    #[test]
    fn list_workspace_children_keeps_directory_when_listable_walk_hits_visit_budget() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        let wide = workspace.path().join("wide");
        fs::create_dir(&wide).expect("create wide");
        for index in 0..=LISTABLE_DESCENDANT_MAX_VISITS {
            let child = wide.join(format!("d{index}"));
            fs::create_dir(&child).expect("create child");
            fs::write(child.join("ignored.txt"), "ignored\n").expect("write ignored");
        }
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        let hidden =
            list_workspace_children(workspace_path(&workspace), String::new(), true).unwrap();
        let hidden_names = entry_names(&hidden);
        assert!(hidden_names.contains(&"wide"));
        assert!(hidden_names.contains(&"readme.md"));
        let wide_entry = hidden.iter().find(|entry| entry.name == "wide").unwrap();
        assert!(wide_entry.has_children_hint);
    }

    #[cfg(windows)]
    #[test]
    fn list_workspace_children_skips_reserved_device_name_entries() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let verbatim = format!(r"\\?\{}\nul", workspace.path().display());
        fs::write(&verbatim, "").expect("write reserved-name file");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        for hide_ignored in [true, false] {
            let entries =
                list_workspace_children(workspace_path(&workspace), String::new(), hide_ignored)
                    .unwrap();
            assert_eq!(entry_names(&entries), vec!["readme.md"]);
        }
        let _ = fs::remove_file(&verbatim);
    }

    #[test]
    fn list_workspace_children_always_hides_protected_git_directory() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::create_dir(workspace.path().join(".git")).expect("create git dir");
        fs::write(workspace.path().join(".git/config"), "protected").expect("write git config");
        fs::write(workspace.path().join("readme.md"), "# Alera\n").expect("write readme");

        for hide_ignored in [true, false] {
            let entries =
                list_workspace_children(workspace_path(&workspace), String::new(), hide_ignored)
                    .unwrap();
            let names = entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>();

            assert!(names.contains(&"readme.md"));
            assert!(!names.contains(&".git"));
        }
    }
}
