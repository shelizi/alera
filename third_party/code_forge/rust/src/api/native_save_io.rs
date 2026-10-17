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
pub fn guarded_replace_workspace_file(
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
}
