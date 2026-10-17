//! Experimental guarded replace; not wired into production Save.
use std::{
    fs,
    io::Write,
    path::{Component, Path},
    time::UNIX_EPOCH,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Saved,
    Conflict,
    InvalidPath,
    Protected,
    Unsupported,
    Io,
}
pub struct SaveOutcome {
    pub status: SaveStatus,
    pub token: String,
    pub size: u64,
}
fn token(m: &fs::Metadata) -> String {
    let t = m
        .modified()
        .ok()
        .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
        .map(|v| v.as_millis())
        .unwrap_or_default();
    format!("{}:{t}", m.len())
}
pub(crate) fn guarded_replace_workspace_file(
    root: String,
    relative: String,
    bytes: Vec<u8>,
    expected_token: String,
    overwrite: bool,
) -> SaveOutcome {
    let run = || -> Result<(String, u64), SaveStatus> {
        let rel = Path::new(&relative);
        if rel.as_os_str().is_empty() {
            return Err(SaveStatus::InvalidPath);
        }
        for c in rel.components() {
            match c {
                Component::Normal(v)
                    if [".git", ".hg", ".svn"]
                        .iter()
                        .any(|x| v.to_str().is_some_and(|s| s.eq_ignore_ascii_case(x))) =>
                {
                    return Err(SaveStatus::Protected)
                }
                Component::Normal(_) | Component::CurDir => {}
                _ => return Err(SaveStatus::InvalidPath),
            }
        }
        if bytes.len() > 10 * 1024 * 1024 {
            return Err(SaveStatus::Unsupported);
        }
        let base = fs::canonicalize(root).map_err(|_| SaveStatus::InvalidPath)?;
        let dest = fs::canonicalize(base.join(rel)).map_err(|_| SaveStatus::InvalidPath)?;
        if !dest.starts_with(&base) {
            return Err(SaveStatus::InvalidPath);
        }
        let inner = dest
            .strip_prefix(&base)
            .map_err(|_| SaveStatus::InvalidPath)?;
        for component in inner.components() {
            if let Component::Normal(name) = component {
                if [".git", ".hg", ".svn"].iter().any(|x| {
                    name.to_str()
                        .is_some_and(|value| value.eq_ignore_ascii_case(x))
                }) {
                    return Err(SaveStatus::Protected);
                }
            }
        }
        let before = fs::metadata(&dest).map_err(|_| SaveStatus::Io)?;
        if !before.is_file() || before.permissions().readonly() {
            return Err(SaveStatus::Unsupported);
        }
        let current = token(&before);
        if !overwrite && current != expected_token {
            return Err(SaveStatus::Conflict);
        }
        let parent = dest.parent().ok_or(SaveStatus::InvalidPath)?;
        let temp = parent.join(format!(
            ".alera-native-save-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|_| SaveStatus::Io)?;
        let result = (|| -> Result<(), SaveStatus> {
            f.write_all(&bytes).map_err(|_| SaveStatus::Io)?;
            f.sync_all().map_err(|_| SaveStatus::Io)?;
            fs::set_permissions(&temp, before.permissions()).map_err(|_| SaveStatus::Io)?;
            drop(f);
            if fs::canonicalize(base.join(rel)).map_err(|_| SaveStatus::Conflict)? != dest {
                return Err(SaveStatus::Conflict);
            }
            if token(&fs::metadata(&dest).map_err(|_| SaveStatus::Conflict)?) != current {
                return Err(SaveStatus::Conflict);
            }
            fs::rename(&temp, &dest).map_err(|_| SaveStatus::Io)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        let after = fs::metadata(dest).map_err(|_| SaveStatus::Io)?;
        Ok((token(&after), after.len()))
    };
    match run() {
        Ok((token, size)) => SaveOutcome {
            status: SaveStatus::Saved,
            token,
            size,
        },
        Err(status) => SaveOutcome {
            status,
            token: String::new(),
            size: 0,
        },
    }
}

pub(crate) fn save_rope(
    rope: &ropey::Rope,
    root: String,
    relative: String,
    expected_token: String,
    overwrite: bool,
    tab_size: i32,
    encoding: super::workspace_source::NativeWorkspaceTextEncoding,
) -> SaveOutcome {
    // Reuse the workspace's existing path and text decoding rules.
    let source = match super::workspace_source::open_workspace_source(
        &root,
        &relative,
        tab_size,
        Some(encoding),
    ) {
        Ok(source) => source,
        Err(_) => {
            return SaveOutcome {
                status: SaveStatus::InvalidPath,
                token: String::new(),
                size: 0,
            }
        }
    };
    if !overwrite && source.info.content_token != expected_token {
        return SaveOutcome {
            status: SaveStatus::Conflict,
            token: String::new(),
            size: 0,
        };
    }
    let root_path = match fs::canonicalize(&root) {
        Ok(path) => path,
        Err(_) => {
            return SaveOutcome {
                status: SaveStatus::InvalidPath,
                token: String::new(),
                size: 0,
            }
        }
    };
    let path = match fs::canonicalize(root_path.join(&relative)) {
        Ok(path) if path.starts_with(&root_path) => path,
        _ => {
            return SaveOutcome {
                status: SaveStatus::InvalidPath,
                token: String::new(),
                size: 0,
            }
        }
    };
    let original = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return SaveOutcome {
                status: SaveStatus::Io,
                token: String::new(),
                size: 0,
            }
        }
    };
    let raw = match super::workspace_source::decode_with_encoding(&original, encoding) {
        Ok(raw) => raw,
        Err(_) => {
            return SaveOutcome {
                status: SaveStatus::Unsupported,
                token: String::new(),
                size: 0,
            }
        }
    };
    let display = super::workspace_source::expand_tabs(&raw, tab_size);
    let bytes = if rope.chars().eq(display.chars()) {
        original // No-op save must preserve the original byte sequence exactly.
    } else {
        match super::native_save::snapshot_bytes(rope, &raw, &display, encoding) {
            Ok(bytes) => bytes,
            Err(_) => {
                return SaveOutcome {
                    status: SaveStatus::Unsupported,
                    token: String::new(),
                    size: 0,
                }
            }
        }
    };
    guarded_replace_workspace_file(root, relative, bytes, expected_token, overwrite)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escape() {
        assert_eq!(
            guarded_replace_workspace_file(".".into(), "../x".into(), vec![], "".into(), false)
                .status,
            SaveStatus::InvalidPath
        );
    }
    #[test]
    fn protected() {
        assert_eq!(
            guarded_replace_workspace_file(
                ".".into(),
                ".GIT/config".into(),
                vec![],
                "".into(),
                false
            )
            .status,
            SaveStatus::Protected
        );
    }

    #[test]
    fn actual_replace_and_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("content.txt");
        fs::write(&file, b"initial").unwrap();
        let expected = token(&fs::metadata(&file).unwrap());
        let root = dir.path().to_string_lossy().into_owned();
        let saved = guarded_replace_workspace_file(
            root.clone(),
            "content.txt".into(),
            b"changed content".to_vec(),
            expected.clone(),
            false,
        );
        assert_eq!(saved.status, SaveStatus::Saved);
        assert_eq!(saved.size, 15);
        assert_eq!(fs::read(&file).unwrap(), b"changed content");
        let conflicted = guarded_replace_workspace_file(
            root,
            "content.txt".into(),
            b"should not overwrite".to_vec(),
            expected,
            false,
        );
        assert_eq!(conflicted.status, SaveStatus::Conflict);
        assert_eq!(fs::read(&file).unwrap(), b"changed content");
    }
    #[test]
    fn overwrite_requires_explicit_option() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("content.txt");
        fs::write(&file, b"initial").unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let saved = guarded_replace_workspace_file(
            root,
            "content.txt".into(),
            b"replacement".to_vec(),
            "stale".into(),
            true,
        );
        assert_eq!(saved.status, SaveStatus::Saved);
        assert_eq!(fs::read(&file).unwrap(), b"replacement");
    }

    #[test]
    fn rope_snapshot_preserves_utf8_bom_and_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.txt");
        fs::write(&path, b"\xef\xbb\xbf\talpha\n\tbeta\n").unwrap();
        let start = token(&fs::metadata(&path).unwrap());
        let rope = ropey::Rope::from_str("    alpha\n    edited\n");
        let result = save_rope(
            &rope,
            dir.path().to_string_lossy().into_owned(),
            "source.txt".into(),
            start,
            false,
            4,
            super::super::workspace_source::NativeWorkspaceTextEncoding::Utf8Bom,
        );
        assert_eq!(result.status, SaveStatus::Saved);
        assert_eq!(
            fs::read(path).unwrap(),
            b"\xef\xbb\xbf\talpha\n    edited\n"
        );
    }
    #[test]
    fn rope_snapshot_preserves_original_bytes_if_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.txt");
        let original = b"\tfoo\r\n\tbar\r\n";
        fs::write(&path, original).unwrap();
        let start = token(&fs::metadata(&path).unwrap());
        let rope = ropey::Rope::from_str("    foo\r\n    bar\r\n");
        let result = save_rope(
            &rope,
            dir.path().to_string_lossy().into_owned(),
            "source.txt".into(),
            start,
            false,
            4,
            super::super::workspace_source::NativeWorkspaceTextEncoding::Utf8,
        );
        assert_eq!(result.status, SaveStatus::Saved);
        assert_eq!(fs::read(path).unwrap(), original);
    }
}
