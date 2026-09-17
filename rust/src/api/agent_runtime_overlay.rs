use serde_json::json;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_WARNINGS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub enum AgentRuntimeOverlayWriteMode {
    Replace,
    CreateIfMissing,
}

#[derive(Clone, Debug)]
pub struct AgentRuntimeOverlayManagedFile {
    pub path: String,
    pub allowed_root: String,
    pub content: String,
    pub write_mode: AgentRuntimeOverlayWriteMode,
    pub executable: bool,
}

#[derive(Clone, Debug)]
pub struct AgentRuntimeOverlayRequest {
    pub overlay_root: Option<String>,
    pub overlay_path: Option<String>,
    pub mirror_path: Option<String>,
    pub source_path: Option<String>,
    pub managed_subdirectory: Option<String>,
    pub managed_file_names: Vec<String>,
    pub managed_files: Vec<AgentRuntimeOverlayManagedFile>,
}

#[derive(Clone, Debug, Default)]
pub struct AgentRuntimeOverlayResult {
    pub source_exists: bool,
    pub linked_count: u64,
    pub copied_count: u64,
    pub written_count: u64,
    pub removed_count: u64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AgentRuntimeOverlayCleanupTarget {
    pub overlay_root: String,
    pub overlay_path: String,
}

#[derive(Clone, Debug, Default)]
pub struct AgentRuntimeOverlayCleanupResult {
    pub removed_count: u64,
    pub warnings: Vec<String>,
}

pub fn prepare_agent_runtime_overlay(
    request: AgentRuntimeOverlayRequest,
) -> Result<AgentRuntimeOverlayResult, String> {
    let cleanup_overlay = overlay_pair(&request)?;
    let result = prepare_agent_runtime_overlay_with_linker(request, create_resource_link);
    if result.is_err() {
        if let Some((overlay_root, overlay_path)) = cleanup_overlay {
            let _ = safe_remove_overlay(&overlay_path, &overlay_root);
        }
    }
    result
}

pub fn clear_agent_runtime_overlays(
    targets: Vec<AgentRuntimeOverlayCleanupTarget>,
) -> Result<AgentRuntimeOverlayCleanupResult, String> {
    let mut result = AgentRuntimeOverlayCleanupResult::default();
    for target in targets {
        match safe_remove_overlay(
            Path::new(&target.overlay_path),
            Path::new(&target.overlay_root),
        ) {
            Ok(removed) => result.removed_count += removed,
            Err(error) => push_warning(
                &mut result.warnings,
                format!("failed to clear {}: {error}", target.overlay_path),
            ),
        }
    }
    Ok(result)
}

fn prepare_agent_runtime_overlay_with_linker<F>(
    request: AgentRuntimeOverlayRequest,
    mut create_link: F,
) -> Result<AgentRuntimeOverlayResult, String>
where
    F: FnMut(&Path, &Path) -> Result<(), String>,
{
    let mut result = AgentRuntimeOverlayResult::default();
    let overlay = overlay_pair(&request)?;
    let mirror_path = mirror_path(&request, overlay.as_ref())?;

    if let Some((overlay_root, overlay_path)) = overlay.as_ref() {
        result.removed_count += safe_remove_overlay(overlay_path, overlay_root)?;
        fs::create_dir_all(overlay_path).map_err(|error| {
            format!(
                "failed to create overlay directory {}: {error}",
                overlay_path.display()
            )
        })?;
    }
    if let Some(mirror_path) = mirror_path.as_ref() {
        fs::create_dir_all(mirror_path).map_err(|error| {
            format!(
                "failed to create overlay mirror directory {}: {error}",
                mirror_path.display()
            )
        })?;
    }

    if let Some(source) = request.source_path.as_deref().map(Path::new) {
        result.source_exists = entity_exists(source)?;
        if result.source_exists {
            if let Some(mirror_path) = mirror_path.as_ref() {
                mirror_source_directory(
                    source,
                    mirror_path,
                    request.managed_subdirectory.as_deref(),
                    &request.managed_file_names,
                    &mut create_link,
                    &mut result,
                )?;
            }
        }
    }

    for managed_file in &request.managed_files {
        if write_managed_file(managed_file)? {
            result.written_count += 1;
        }
    }

    Ok(result)
}

fn overlay_pair(
    request: &AgentRuntimeOverlayRequest,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    match (&request.overlay_root, &request.overlay_path) {
        (None, None) => Ok(None),
        (Some(root), Some(path)) => {
            let root = normalize_absolute(Path::new(root))?;
            let path = normalize_absolute(Path::new(path))?;
            ensure_strict_descendant(&path, &root, "overlay path")?;
            Ok(Some((root, path)))
        }
        _ => Err("overlay_root and overlay_path must either both be set or both be absent".into()),
    }
}

fn mirror_path(
    request: &AgentRuntimeOverlayRequest,
    overlay: Option<&(PathBuf, PathBuf)>,
) -> Result<Option<PathBuf>, String> {
    let Some((_, overlay_path)) = overlay else {
        if request.mirror_path.is_some() {
            return Err("mirror_path requires overlay_root and overlay_path".into());
        }
        return Ok(None);
    };
    let mirror = request
        .mirror_path
        .as_deref()
        .map(Path::new)
        .map(normalize_absolute)
        .transpose()?
        .unwrap_or_else(|| overlay_path.clone());
    if mirror != *overlay_path {
        ensure_strict_descendant(&mirror, overlay_path, "mirror path")?;
    }
    Ok(Some(mirror))
}

fn mirror_source_directory<F>(
    source_path: &Path,
    overlay_path: &Path,
    managed_subdirectory: Option<&str>,
    managed_file_names: &[String],
    create_link: &mut F,
    result: &mut AgentRuntimeOverlayResult,
) -> Result<(), String>
where
    F: FnMut(&Path, &Path) -> Result<(), String>,
{
    let metadata = fs::metadata(source_path)
        .map_err(|error| format!("failed to stat source {}: {error}", source_path.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "overlay source is not a directory: {}",
            source_path.display()
        ));
    }

    let entries = read_directory_entries(source_path)?;
    for entry in entries {
        let entry_name = entry.file_name();
        let entry_path = entry.path();
        if managed_subdirectory.is_some_and(|managed| entry_name == managed)
            && fs::metadata(&entry_path)
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false)
        {
            let target_directory = overlay_path.join(&entry_name);
            fs::create_dir_all(&target_directory).map_err(|error| {
                format!(
                    "failed to create managed overlay directory {}: {error}",
                    target_directory.display()
                )
            })?;
            for child in read_directory_entries(&entry_path)? {
                let child_name = child.file_name();
                let child_name_lossy = child_name.to_string_lossy();
                if managed_file_names
                    .iter()
                    .any(|name| child_name_lossy == name.as_str())
                {
                    continue;
                }
                mirror_entry(
                    &child.path(),
                    &target_directory.join(&child_name),
                    overlay_path,
                    create_link,
                    result,
                )?;
            }
            continue;
        }
        mirror_entry(
            &entry_path,
            &overlay_path.join(entry_name),
            overlay_path,
            create_link,
            result,
        )?;
    }
    Ok(())
}

fn mirror_entry<F>(
    source_path: &Path,
    target_path: &Path,
    overlay_path: &Path,
    create_link: &mut F,
    result: &mut AgentRuntimeOverlayResult,
) -> Result<(), String>
where
    F: FnMut(&Path, &Path) -> Result<(), String>,
{
    match create_link(source_path, target_path) {
        Ok(()) => {
            result.linked_count += 1;
            Ok(())
        }
        Err(link_error) => {
            push_warning(
                &mut result.warnings,
                format!(
                    "link {} -> {} failed; copied instead: {link_error}",
                    target_path.display(),
                    source_path.display()
                ),
            );
            copy_entity_preserving_links(source_path, target_path, &mut result.copied_count)?;
            write_copied_resource_marker(overlay_path, target_path, source_path)?;
            Ok(())
        }
    }
}

fn read_directory_entries(path: &Path) -> Result<Vec<fs::DirEntry>, String> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("failed to list {}: {error}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

fn create_resource_link(source_path: &Path, target_path: &Path) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source_path, target_path).map_err(|error| {
            format!(
                "failed to create symlink {} -> {}: {error}",
                target_path.display(),
                source_path.display()
            )
        })
    }

    #[cfg(windows)]
    {
        let is_directory = fs::metadata(source_path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let operation = if is_directory {
            std::os::windows::fs::symlink_dir(source_path, target_path)
        } else {
            std::os::windows::fs::symlink_file(source_path, target_path)
        };
        operation.map_err(|error| {
            format!(
                "failed to create symlink {} -> {}: {error}",
                target_path.display(),
                source_path.display()
            )
        })
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source_path, target_path);
        Err("resource links are unsupported on this platform".into())
    }
}

fn copy_entity_preserving_links(
    source_path: &Path,
    target_path: &Path,
    copied_count: &mut u64,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source_path)
        .map_err(|error| format!("failed to stat {}: {error}", source_path.display()))?;
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        let link_target = fs::read_link(source_path)
            .map_err(|error| format!("failed to read link {}: {error}", source_path.display()))?;
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        create_preserved_symlink(source_path, &link_target, target_path)?;
        *copied_count += 1;
        return Ok(());
    }

    if file_type.is_dir() {
        fs::create_dir_all(target_path)
            .map_err(|error| format!("failed to create {}: {error}", target_path.display()))?;
        *copied_count += 1;
        for entry in read_directory_entries(source_path)? {
            copy_entity_preserving_links(
                &entry.path(),
                &target_path.join(entry.file_name()),
                copied_count,
            )?;
        }
        return Ok(());
    }

    if file_type.is_file() {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        fs::copy(source_path, target_path).map_err(|error| {
            format!(
                "failed to copy {} to {}: {error}",
                source_path.display(),
                target_path.display()
            )
        })?;
        *copied_count += 1;
        return Ok(());
    }

    Err(format!(
        "unsupported filesystem entity in overlay source: {}",
        source_path.display()
    ))
}

fn create_preserved_symlink(
    source_path: &Path,
    link_target: &Path,
    target_path: &Path,
) -> Result<(), String> {
    #[cfg(unix)]
    {
        let _ = source_path;
        std::os::unix::fs::symlink(link_target, target_path).map_err(|error| {
            format!(
                "failed to preserve link {} -> {}: {error}",
                target_path.display(),
                link_target.display()
            )
        })
    }

    #[cfg(windows)]
    {
        let is_directory = fs::metadata(source_path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let operation = if is_directory {
            std::os::windows::fs::symlink_dir(link_target, target_path)
        } else {
            std::os::windows::fs::symlink_file(link_target, target_path)
        };
        operation.map_err(|error| {
            format!(
                "failed to preserve link {} -> {}: {error}",
                target_path.display(),
                link_target.display()
            )
        })
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source_path, link_target, target_path);
        Err("symbolic links are unsupported on this platform".into())
    }
}

fn write_copied_resource_marker(
    overlay_path: &Path,
    target_path: &Path,
    source_path: &Path,
) -> Result<(), String> {
    let relative = target_path.strip_prefix(overlay_path).map_err(|_| {
        format!(
            "copied resource target {} escaped overlay {}",
            target_path.display(),
            overlay_path.display()
        )
    })?;
    let relative_text = relative.to_string_lossy();
    let encoded = base64_url_no_padding(relative_text.as_bytes());
    let marker = overlay_path
        .join(".alera-copied-resources")
        .join(format!("{encoded}.json"));
    let payload = json!({
        "sourcePath": source_path.to_string_lossy(),
        "targetPath": target_path.to_string_lossy(),
    });
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&marker, format!("{payload}\n"))
        .map_err(|error| format!("failed to write {}: {error}", marker.display()))
}

fn write_managed_file(file: &AgentRuntimeOverlayManagedFile) -> Result<bool, String> {
    let allowed_root = normalize_absolute(Path::new(&file.allowed_root))?;
    let path = normalize_absolute(Path::new(&file.path))?;
    ensure_strict_descendant(&path, &allowed_root, "managed file path")?;

    if matches!(
        file.write_mode,
        AgentRuntimeOverlayWriteMode::CreateIfMissing
    ) {
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => return Ok(false),
            Ok(_) => {
                return Err(format!(
                    "managed file path exists but is not a file: {}",
                    path.display()
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if fs::symlink_metadata(&path).is_ok() {
                    return Err(format!(
                        "managed file path is an unresolved link: {}",
                        path.display()
                    ));
                }
            }
            Err(error) => {
                return Err(format!("failed to stat {}: {error}", path.display()));
            }
        }
    } else {
        remove_entity_if_exists(&path)?;
    }

    let parent = path
        .parent()
        .ok_or_else(|| format!("managed file has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

    let tmp_path = unique_temp_path(parent);
    fs::write(&tmp_path, file.content.as_bytes())
        .map_err(|error| format!("failed to write {}: {error}", tmp_path.display()))?;
    if file.executable {
        set_executable(&tmp_path)?;
    }
    match fs::rename(&tmp_path, &path) {
        Ok(()) => Ok(true),
        Err(error) => {
            let _ = fs::remove_file(&tmp_path);
            Err(format!(
                "failed to rename {} to {}: {error}",
                tmp_path.display(),
                path.display()
            ))
        }
    }
}

fn unique_temp_path(parent: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{nanos}-{}.tmp", std::process::id()))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to chmod {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn safe_remove_overlay(overlay_path: &Path, overlay_root: &Path) -> Result<u64, String> {
    let root = normalize_absolute(overlay_root)?;
    let target = normalize_absolute(overlay_path)?;
    ensure_strict_descendant(&target, &root, "overlay path")?;
    remove_tree_counted(&target)
}

fn remove_tree_counted(path: &Path) -> Result<u64, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("failed to stat {}: {error}", path.display())),
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() || file_type.is_file() {
        remove_file_or_link(path)?;
        return Ok(1);
    }
    if file_type.is_dir() {
        let mut count = 1;
        for entry in read_directory_entries(path)? {
            count += remove_tree_counted(&entry.path())?;
        }
        fs::remove_dir(path)
            .map_err(|error| format!("failed to remove directory {}: {error}", path.display()))?;
        return Ok(count);
    }
    remove_file_or_link(path)?;
    Ok(1)
}

fn remove_entity_if_exists(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to stat {}: {error}", path.display())),
    };
    let file_type = metadata.file_type();
    if file_type.is_dir() && !file_type.is_symlink() {
        fs::remove_dir_all(path)
            .map_err(|error| format!("failed to remove directory {}: {error}", path.display()))
    } else {
        remove_file_or_link(path)
    }
}

fn remove_file_or_link(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(file_error) => fs::remove_dir(path).map_err(|dir_error| {
            format!(
                "failed to remove {} as file ({file_error}) or directory ({dir_error})",
                path.display()
            )
        }),
    }
}

fn entity_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to stat {}: {error}", path.display())),
    }
}

fn normalize_absolute(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn ensure_strict_descendant(path: &Path, root: &Path, label: &str) -> Result<(), String> {
    let path_components = comparable_components(path);
    let root_components = comparable_components(root);
    if path_components.len() <= root_components.len()
        || !path_components
            .iter()
            .zip(&root_components)
            .all(|(left, right)| components_equal(left, right))
    {
        return Err(format!(
            "{label} must be strictly contained by root: {} not under {}",
            path.display(),
            root.display()
        ));
    }
    Ok(())
}

fn comparable_components(path: &Path) -> Vec<OsString> {
    path.components()
        .map(|component| component.as_os_str().to_os_string())
        .collect()
}

#[cfg(windows)]
fn components_equal(left: &OsString, right: &OsString) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(not(windows))]
fn components_equal(left: &OsString, right: &OsString) -> bool {
    left == right
}

fn push_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < MAX_WARNINGS {
        warnings.push(warning);
    }
}

fn base64_url_no_padding(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | bytes[index + 2] as u32;
        output.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        output.push(TABLE[((chunk >> 6) & 0x3f) as usize] as char);
        output.push(TABLE[(chunk & 0x3f) as usize] as char);
        index += 3;
    }
    match bytes.len() - index {
        1 => {
            let chunk = (bytes[index] as u32) << 16;
            output.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
            output.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        }
        2 => {
            let chunk = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            output.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
            output.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
            output.push(TABLE[((chunk >> 6) & 0x3f) as usize] as char);
        }
        _ => {}
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn request_for(root: &Path, source: Option<&Path>) -> AgentRuntimeOverlayRequest {
        let overlay_root = root.join("overlays");
        let overlay_path = overlay_root.join("session");
        AgentRuntimeOverlayRequest {
            overlay_root: Some(overlay_root.to_string_lossy().into_owned()),
            overlay_path: Some(overlay_path.to_string_lossy().into_owned()),
            mirror_path: None,
            source_path: source.map(|path| path.to_string_lossy().into_owned()),
            managed_subdirectory: Some("plugins".into()),
            managed_file_names: vec!["alera.js".into()],
            managed_files: vec![AgentRuntimeOverlayManagedFile {
                path: overlay_path
                    .join("plugins/alera.js")
                    .to_string_lossy()
                    .into_owned(),
                allowed_root: overlay_path.to_string_lossy().into_owned(),
                content: "managed\n".into(),
                write_mode: AgentRuntimeOverlayWriteMode::Replace,
                executable: false,
            }],
        }
    }

    #[test]
    fn prepares_overlay_without_source_and_writes_managed_file() {
        let root = tempfile::tempdir().unwrap();
        let request = request_for(root.path(), None);
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        let result = prepare_agent_runtime_overlay(request).unwrap();

        assert!(!result.source_exists);
        assert_eq!(result.written_count, 1);
        assert_eq!(
            fs::read_to_string(overlay.join("plugins/alera.js")).unwrap(),
            "managed\n"
        );
    }

    #[test]
    fn missing_source_is_not_an_error() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        let result =
            prepare_agent_runtime_overlay(request_for(root.path(), Some(&missing))).unwrap();
        assert!(!result.source_exists);
    }

    #[test]
    fn mirrors_nested_tree_and_excludes_managed_file() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("plugins")).unwrap();
        fs::create_dir_all(source.join("nested/deeper")).unwrap();
        fs::write(source.join("plugins/user.js"), "user").unwrap();
        fs::write(source.join("plugins/alera.js"), "user-owned").unwrap();
        fs::write(source.join("nested/deeper/value.txt"), "nested").unwrap();
        let mut request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        let result = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();

        assert!(result.source_exists);
        assert!(result.copied_count >= 4);
        assert_eq!(
            fs::read_to_string(overlay.join("plugins/user.js")).unwrap(),
            "user"
        );
        assert_eq!(
            fs::read_to_string(overlay.join("plugins/alera.js")).unwrap(),
            "managed\n"
        );
        assert_eq!(
            fs::read_to_string(overlay.join("nested/deeper/value.txt")).unwrap(),
            "nested"
        );
        assert!(overlay.join(".alera-copied-resources").is_dir());

        request.managed_files[0].content = "managed-v2\n".into();
        prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
            .unwrap();
        assert_eq!(
            fs::read_to_string(overlay.join("plugins/alera.js")).unwrap(),
            "managed-v2\n"
        );
    }

    #[test]
    fn mirrors_into_nested_mirror_path_without_changing_cleanup_root() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("settings.json"), "amp-settings").unwrap();
        let mut request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());
        let mirror = overlay.join("xdg/amp");
        request.mirror_path = Some(mirror.to_string_lossy().into_owned());
        request.managed_files[0].path = mirror
            .join("plugins/alera.js")
            .to_string_lossy()
            .into_owned();
        request.managed_files[0].allowed_root = mirror.to_string_lossy().into_owned();

        prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
            .unwrap();

        assert_eq!(
            fs::read_to_string(mirror.join("settings.json")).unwrap(),
            "amp-settings"
        );
        assert_eq!(
            fs::read_to_string(mirror.join("plugins/alera.js")).unwrap(),
            "managed\n"
        );
        assert!(!overlay.join("settings.json").exists());
    }

    #[test]
    fn removes_stale_overlay_before_rebuild() {
        let root = tempfile::tempdir().unwrap();
        let request = request_for(root.path(), None);
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());
        fs::create_dir_all(overlay.join("stale/nested")).unwrap();
        fs::write(overlay.join("stale/nested/old.txt"), "old").unwrap();

        let result = prepare_agent_runtime_overlay(request).unwrap();

        assert!(result.removed_count >= 4);
        assert!(!overlay.join("stale").exists());
    }

    #[test]
    fn rejects_overlay_outside_declared_root() {
        let root = tempfile::tempdir().unwrap();
        let mut request = request_for(root.path(), None);
        request.overlay_path = Some(root.path().join("outside").to_string_lossy().into_owned());

        let error = prepare_agent_runtime_overlay(request).unwrap_err();

        assert!(error.contains("strictly contained"));
    }

    #[test]
    fn rejects_managed_file_outside_allowed_root() {
        let root = tempfile::tempdir().unwrap();
        let mut request = request_for(root.path(), None);
        request.managed_files[0].path = root
            .path()
            .join("escaped.txt")
            .to_string_lossy()
            .into_owned();

        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());
        let error = prepare_agent_runtime_overlay(request).unwrap_err();

        assert!(error.contains("managed file path"));
        assert!(
            !overlay.exists(),
            "failed transactions must roll back the overlay"
        );
    }

    #[test]
    fn create_if_missing_preserves_existing_regular_file() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("runtime");
        fs::create_dir_all(&allowed).unwrap();
        let target = allowed.join("settings.json");
        fs::write(&target, "user\n").unwrap();
        let request = AgentRuntimeOverlayRequest {
            overlay_root: None,
            overlay_path: None,
            mirror_path: None,
            source_path: None,
            managed_subdirectory: None,
            managed_file_names: Vec::new(),
            managed_files: vec![AgentRuntimeOverlayManagedFile {
                path: target.to_string_lossy().into_owned(),
                allowed_root: allowed.to_string_lossy().into_owned(),
                content: "{}\n".into(),
                write_mode: AgentRuntimeOverlayWriteMode::CreateIfMissing,
                executable: false,
            }],
        };

        let result = prepare_agent_runtime_overlay(request).unwrap();

        assert_eq!(result.written_count, 0);
        assert_eq!(fs::read_to_string(target).unwrap(), "user\n");
    }

    #[test]
    fn create_if_missing_preserves_link_to_existing_file_when_supported() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("runtime");
        fs::create_dir_all(&allowed).unwrap();
        let source = root.path().join("user-settings.json");
        fs::write(&source, "user-settings\n").unwrap();
        let target = allowed.join("settings.json");
        if create_test_symlink(&source, &target).is_err() {
            return;
        }
        let request = AgentRuntimeOverlayRequest {
            overlay_root: None,
            overlay_path: None,
            mirror_path: None,
            source_path: None,
            managed_subdirectory: None,
            managed_file_names: Vec::new(),
            managed_files: vec![AgentRuntimeOverlayManagedFile {
                path: target.to_string_lossy().into_owned(),
                allowed_root: allowed.to_string_lossy().into_owned(),
                content: "{}\n".into(),
                write_mode: AgentRuntimeOverlayWriteMode::CreateIfMissing,
                executable: false,
            }],
        };

        let result = prepare_agent_runtime_overlay(request).unwrap();

        assert_eq!(result.written_count, 0);
        assert_eq!(fs::read_to_string(target).unwrap(), "user-settings\n");
        assert_eq!(fs::read_to_string(source).unwrap(), "user-settings\n");
    }

    #[test]
    fn can_prepare_direct_managed_file_without_overlay() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("pi-agent");
        fs::create_dir_all(&source).unwrap();
        let target = source.join("extensions/alera.ts");
        let request = AgentRuntimeOverlayRequest {
            overlay_root: None,
            overlay_path: None,
            mirror_path: None,
            source_path: Some(source.to_string_lossy().into_owned()),
            managed_subdirectory: None,
            managed_file_names: Vec::new(),
            managed_files: vec![AgentRuntimeOverlayManagedFile {
                path: target.to_string_lossy().into_owned(),
                allowed_root: source.to_string_lossy().into_owned(),
                content: "managed pi\n".into(),
                write_mode: AgentRuntimeOverlayWriteMode::Replace,
                executable: false,
            }],
        };

        let result = prepare_agent_runtime_overlay(request).unwrap();

        assert!(result.source_exists);
        assert_eq!(result.written_count, 1);
        assert_eq!(fs::read_to_string(target).unwrap(), "managed pi\n");
    }

    #[test]
    fn fallback_copy_preserves_symbolic_link_when_supported() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(&source).unwrap();
        let target_file = root.path().join("target.txt");
        fs::write(&target_file, "target").unwrap();
        let source_link = source.join("linked.txt");
        if create_test_symlink(&target_file, &source_link).is_err() {
            return;
        }
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
            .unwrap();

        let copied_link = overlay.join("linked.txt");
        assert!(fs::symlink_metadata(&copied_link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_link(copied_link).unwrap(), target_file);
    }

    #[test]
    fn records_link_success_without_copy_marker() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("config.json"), "value").unwrap();
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        let result = prepare_agent_runtime_overlay_with_linker(request, |source, target| {
            fs::hard_link(source, target).map_err(|error| error.to_string())
        })
        .unwrap();

        assert_eq!(result.linked_count, 1);
        assert_eq!(result.copied_count, 0);
        assert_eq!(
            fs::read_to_string(overlay.join("config.json")).unwrap(),
            "value"
        );
        assert!(!overlay.join(".alera-copied-resources").exists());
    }

    #[test]
    fn clear_overlays_removes_only_strict_descendants() {
        let root = tempfile::tempdir().unwrap();
        let overlay_root = root.path().join("overlays");
        let overlay = overlay_root.join("session");
        fs::create_dir_all(&overlay).unwrap();
        fs::write(overlay.join("value.txt"), "value").unwrap();
        let protected = root.path().join("protected.txt");
        fs::write(&protected, "keep").unwrap();

        let result = clear_agent_runtime_overlays(vec![AgentRuntimeOverlayCleanupTarget {
            overlay_root: overlay_root.to_string_lossy().into_owned(),
            overlay_path: overlay.to_string_lossy().into_owned(),
        }])
        .unwrap();

        assert!(result.removed_count >= 2);
        assert!(!overlay.exists());
        assert_eq!(fs::read_to_string(protected).unwrap(), "keep");
    }

    #[test]
    fn base64_marker_name_matches_dart_base64_url_without_padding() {
        assert_eq!(
            base64_url_no_padding(b"nested/config.json"),
            "bmVzdGVkL2NvbmZpZy5qc29u"
        );
    }

    #[cfg(windows)]
    #[test]
    fn containment_is_case_insensitive_on_windows() {
        let root = PathBuf::from(r"C:\Users\Test\Overlay");
        let child = PathBuf::from(r"c:\users\test\overlay\session");
        assert!(ensure_strict_descendant(&child, &root, "overlay path").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn executable_managed_file_gets_execute_bits() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("wrappers");
        let target = allowed.join("bin/amp");
        let request = AgentRuntimeOverlayRequest {
            overlay_root: None,
            overlay_path: None,
            mirror_path: None,
            source_path: None,
            managed_subdirectory: None,
            managed_file_names: Vec::new(),
            managed_files: vec![AgentRuntimeOverlayManagedFile {
                path: target.to_string_lossy().into_owned(),
                allowed_root: allowed.to_string_lossy().into_owned(),
                content: "#!/bin/sh\n".into(),
                write_mode: AgentRuntimeOverlayWriteMode::Replace,
                executable: true,
            }],
        };
        prepare_agent_runtime_overlay(request).unwrap();
        assert_eq!(
            fs::metadata(target).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }

    #[test]
    #[ignore = "manual five-sample overlay filesystem benchmark"]
    fn benchmark_small_and_large_fallback_copy() {
        for file_count in [20usize, 2_000] {
            let mut samples = Vec::new();
            for sample in 0..5 {
                let root = tempfile::tempdir().unwrap();
                let source = root.path().join("source");
                fs::create_dir_all(&source).unwrap();
                for index in 0..file_count {
                    let group = source.join(format!("group-{}", index / 100));
                    fs::create_dir_all(&group).unwrap();
                    fs::write(group.join(format!("file-{index}.txt")), b"0123456789abcdef")
                        .unwrap();
                }
                let request = request_for(root.path(), Some(&source));
                let started = Instant::now();
                let result = prepare_agent_runtime_overlay_with_linker(request, |_, _| {
                    Err("benchmark forces copy fallback".into())
                })
                .unwrap();
                let elapsed = started.elapsed();
                eprintln!(
                    "overlay benchmark files={file_count} sample={} elapsed_us={} copied={}",
                    sample + 1,
                    elapsed.as_micros(),
                    result.copied_count
                );
                samples.push(elapsed.as_micros());
            }
            samples.sort_unstable();
            eprintln!(
                "overlay benchmark files={file_count} median_us={}",
                samples[samples.len() / 2]
            );
        }
    }

    fn create_test_symlink(target: &Path, link: &Path) -> Result<(), String> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).map_err(|error| error.to_string())
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).map_err(|error| error.to_string())
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (target, link);
            Err("unsupported".into())
        }
    }
}
