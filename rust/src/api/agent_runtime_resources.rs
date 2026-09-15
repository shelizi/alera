use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Copy)]
enum FingerprintStyle {
    ClaudeLegacy,
    CodexCanonical,
}

#[derive(Debug, PartialEq, Eq)]
enum ResourceRecord {
    Directory {
        path: String,
        modified_micros: u128,
    },
    File {
        path: String,
        size: u64,
        modified_micros: u128,
    },
    Link {
        path: String,
        target: Option<String>,
    },
    Other {
        path: String,
        kind: String,
    },
}

pub fn fingerprint_claude_runtime_resource(source_path: String) -> Result<String, String> {
    fingerprint_runtime_resource(Path::new(&source_path), FingerprintStyle::ClaudeLegacy)
}

pub fn fingerprint_codex_runtime_resource(source_path: String) -> Result<String, String> {
    fingerprint_runtime_resource(Path::new(&source_path), FingerprintStyle::CodexCanonical)
}

fn fingerprint_runtime_resource(
    source_path: &Path,
    style: FingerprintStyle,
) -> Result<String, String> {
    let mut records = Vec::new();
    collect_records(source_path, Path::new(""), style, &mut records)?;
    Ok(hash_serialized_records(&records, style))
}

fn collect_records(
    current_path: &Path,
    relative_path: &Path,
    style: FingerprintStyle,
    records: &mut Vec<ResourceRecord>,
) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(current_path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(style, FingerprintStyle::ClaudeLegacy)
                && error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            records.push(ResourceRecord::Other {
                path: path_string(relative_path),
                kind: "notFound".to_owned(),
            });
            return Ok(());
        }
        Err(error) => {
            return Err(format!(
                "failed to stat {}: {error}",
                current_path.display()
            ))
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        records.push(ResourceRecord::Directory {
            path: path_string(relative_path),
            modified_micros: legacy_modified_micros(&metadata)?,
        });
        let mut children = fs::read_dir(current_path)
            .map_err(|error| format!("failed to list {}: {error}", current_path.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read {}: {error}", current_path.display()))?;
        children.sort_by(|left, right| {
            left.file_name()
                .to_string_lossy()
                .cmp(&right.file_name().to_string_lossy())
        });
        for child in children {
            let child_relative = if relative_path.as_os_str().is_empty() {
                PathBuf::from(child.file_name())
            } else {
                relative_path.join(child.file_name())
            };
            collect_records(&child.path(), &child_relative, style, records)?;
        }
        return Ok(());
    }
    if file_type.is_file() {
        records.push(ResourceRecord::File {
            path: path_string(relative_path),
            size: metadata.len(),
            modified_micros: legacy_modified_micros(&metadata)?,
        });
        return Ok(());
    }
    if file_type.is_symlink() {
        let target = fs::read_link(current_path)
            .ok()
            .map(|target| path_string(&target));
        records.push(ResourceRecord::Link {
            path: path_string(relative_path),
            target,
        });
        return Ok(());
    }
    if matches!(style, FingerprintStyle::CodexCanonical) {
        records.push(ResourceRecord::Other {
            path: path_string(relative_path),
            kind: format!("{:?}", file_type),
        });
    }
    Ok(())
}

fn legacy_modified_micros(metadata: &fs::Metadata) -> Result<u128, String> {
    let duration = metadata
        .modified()
        .map_err(|error| format!("failed to read modified time: {error}"))?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("modified time predates Unix epoch: {error}"))?;
    Ok((duration.as_millis()) * 1_000)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

fn serialize_records(records: &[ResourceRecord], style: FingerprintStyle) -> String {
    let body = records
        .iter()
        .map(|record| serialize_record(record, style))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

fn serialize_record(record: &ResourceRecord, style: FingerprintStyle) -> String {
    match (record, style) {
        (
            ResourceRecord::Directory {
                path,
                modified_micros,
            },
            FingerprintStyle::ClaudeLegacy,
        ) => {
            format!(
                "{{\"path\":{},\"type\":\"directory\",\"modified\":{modified_micros}}}",
                json_string(path)
            )
        }
        (
            ResourceRecord::File {
                path,
                size,
                modified_micros,
            },
            FingerprintStyle::ClaudeLegacy,
        ) => {
            format!(
                "{{\"path\":{},\"type\":\"file\",\"size\":{size},\"modified\":{modified_micros}}}",
                json_string(path)
            )
        }
        (ResourceRecord::Link { path, target }, FingerprintStyle::ClaudeLegacy) => format!(
            "{{\"path\":{},\"type\":\"link\",\"target\":{}}}",
            json_string(path),
            target
                .as_deref()
                .map(json_string)
                .unwrap_or_else(|| "null".to_owned())
        ),
        (ResourceRecord::Other { .. }, FingerprintStyle::ClaudeLegacy) => String::new(),
        (
            ResourceRecord::Directory {
                path,
                modified_micros,
            },
            FingerprintStyle::CodexCanonical,
        ) => {
            format!(
                "{{\"modified\":{modified_micros},\"path\":{},\"type\":\"directory\"}}",
                json_string(path)
            )
        }
        (
            ResourceRecord::File {
                path,
                size,
                modified_micros,
            },
            FingerprintStyle::CodexCanonical,
        ) => {
            format!(
                "{{\"modified\":{modified_micros},\"path\":{},\"size\":{size},\"type\":\"file\"}}",
                json_string(path)
            )
        }
        (ResourceRecord::Link { path, target }, FingerprintStyle::CodexCanonical) => format!(
            "{{\"path\":{},\"target\":{},\"type\":\"link\"}}",
            json_string(path),
            target
                .as_deref()
                .map(json_string)
                .unwrap_or_else(|| "null".to_owned())
        ),
        (ResourceRecord::Other { path, kind }, FingerprintStyle::CodexCanonical) => format!(
            "{{\"path\":{},\"type\":{}}}",
            json_string(path),
            json_string(kind)
        ),
    }
}

fn hash_serialized_records(records: &[ResourceRecord], style: FingerprintStyle) -> String {
    let serialized = serialize_records(records, style);
    format!("sha256:{:x}", Sha256::digest(serialized.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compatibility_record() -> ResourceRecord {
        ResourceRecord::File {
            path: String::new(),
            size: 15,
            modified_micros: 1_767_323_045_123_000,
        }
    }

    #[test]
    fn preserves_claude_legacy_fingerprint_format() {
        assert_eq!(
            hash_serialized_records(&[compatibility_record()], FingerprintStyle::ClaudeLegacy),
            "sha256:a9fa862cdec6b7d664d8dee2faf80f3828f66ecfcbddcb67848eca9d6bfd4f59"
        );
    }

    #[test]
    fn preserves_codex_canonical_fingerprint_format() {
        assert_eq!(
            hash_serialized_records(&[compatibility_record()], FingerprintStyle::CodexCanonical),
            "sha256:b105eea1dcbe75b3f192a566236a62ae9cad43cbea0f2b74231bea442e66dd6c"
        );
    }

    #[test]
    fn sorts_directory_children_like_the_dart_implementation() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("z.txt"), b"z").unwrap();
        fs::write(root.path().join("a.txt"), b"a").unwrap();
        let mut records = Vec::new();

        collect_records(
            root.path(),
            Path::new(""),
            FingerprintStyle::ClaudeLegacy,
            &mut records,
        )
        .unwrap();

        assert!(matches!(&records[1], ResourceRecord::File { path, .. } if path == "a.txt"));
        assert!(matches!(&records[2], ResourceRecord::File { path, .. } if path == "z.txt"));
    }
}
