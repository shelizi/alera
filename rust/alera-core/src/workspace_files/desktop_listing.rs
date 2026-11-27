//! Read-only desktop listing primitives shared by Classic and GPUI.
//! These do not define visibility policy or authorize filesystem operations.

use std::fs;
use std::path::Path;

/// Directory first, Unicode lowercase name second, original name last.
/// Preserve stable ties while computing the lowercase key once per row.
pub fn sort_desktop_directory_entries<T>(entries: &mut [T], key: impl Fn(&T) -> (&str, bool)) {
    entries.sort_by_cached_key(|entry| {
        let (name, is_directory) = key(entry);
        (!is_directory, name.to_lowercase(), name.to_owned())
    });
}

/// Reuse Windows enumeration metadata only for ordinary non-reparse files.
/// The caller must obtain fresh path metadata for all other entries.
/// The supplier is deliberately lazy: no enumeration stat on other platforms.
pub fn reusable_desktop_enumeration_metadata(
    path: &Path,
    metadata: impl FnOnce() -> Option<fs::Metadata>,
) -> Option<fs::Metadata> {
    #[cfg(windows)]
    {
        if is_ordinary_windows_filename(path) {
            metadata().filter(reusable_windows_metadata)
        } else {
            None
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (path, metadata);
        None
    }
}

// Windows directory enumeration already carries file size, timestamps and
// attributes. Limit reuse to ordinary ASCII names; preserve the old path
// stat for device aliases, unusual Win32 names, Unicode, directories,
// symlinks and reparse points. This is listing metadata only: all actual
// reads/writes/mutations still resolve paths and obtain fresh metadata.
#[cfg(windows)]
pub fn is_ordinary_windows_filename(path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_sort_matches_previous_comparator_including_unicode_and_ties() {
        let mut rows = vec![
            ("z", false, 0),
            ("Alpha", false, 1),
            ("alpha", false, 2),
            ("中文", true, 3),
            ("A", true, 4),
            ("\u{00e9}", false, 5),
            ("\u{00c9}", false, 6),
            ("\u{0130}", false, 7),
            ("i\u{0307}", false, 8),
            ("alpha", false, 9),
            ("a", true, 10),
        ];
        let mut expected = rows.clone();
        expected.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
                .then_with(|| a.0.cmp(b.0))
        });
        sort_desktop_directory_entries(&mut rows, |row| (row.0, row.1));
        assert_eq!(rows, expected);
    }

    #[cfg(windows)]
    #[test]
    fn metadata_reuse_rejects_special_names_and_nonregular_entries() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("ordinary.txt");
        fs::write(&file, "hello").unwrap();
        let metadata =
            reusable_desktop_enumeration_metadata(&file, || fs::symlink_metadata(&file).ok())
                .unwrap();
        assert_eq!(metadata.len(), 5);
        for name in [
            "NUL.txt",
            "COM1.rs",
            "LPT9",
            "file.",
            "file name",
            "CONIN$",
            "\u{00e9}.txt",
        ] {
            assert!(
                reusable_desktop_enumeration_metadata(Path::new(name), || panic!(
                    "special names must not invoke supplier"
                ))
                .is_none(),
                "{name}"
            );
        }
        let directory = root.path().join("folder");
        fs::create_dir(&directory).unwrap();
        assert!(
            reusable_desktop_enumeration_metadata(&directory, || fs::symlink_metadata(&directory)
                .ok())
            .is_none()
        );
        assert!(reusable_desktop_enumeration_metadata(&file, || None).is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_keep_path_metadata_policy() {
        assert!(
            reusable_desktop_enumeration_metadata(Path::new("ordinary.txt"), || panic!(
                "supplier must remain lazy"
            ))
            .is_none()
        );
    }
}
