use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_WARNINGS: usize = 64;
const OVERLAY_REUSE_SCHEMA_VERSION: u64 = 1;
const OVERLAY_REUSE_DIRECTORY: &str = ".alera-overlay-reuse";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentRuntimeOverlayReuseManifest {
    schema_version: u64,
    request_fingerprint: String,
    materialized_fingerprint: String,
    source_exists: bool,
    linked_count: u64,
    copied_count: u64,
}

struct AgentRuntimeOverlayInputIdentity {
    fingerprint: String,
    source_exists: bool,
}

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
            let _ = remove_overlay_reuse_manifest(&overlay_root, &overlay_path);
        }
    }
    result
}

pub fn clear_agent_runtime_overlays(
    targets: Vec<AgentRuntimeOverlayCleanupTarget>,
) -> Result<AgentRuntimeOverlayCleanupResult, String> {
    let mut result = AgentRuntimeOverlayCleanupResult::default();
    for target in targets {
        let overlay_root = Path::new(&target.overlay_root);
        let overlay_path = Path::new(&target.overlay_path);
        match safe_remove_overlay(overlay_path, overlay_root) {
            Ok(removed) => {
                result.removed_count += removed;
                if let Err(error) = remove_overlay_reuse_manifest(overlay_root, overlay_path) {
                    push_warning(
                        &mut result.warnings,
                        format!(
                            "failed to clear reuse state for {}: {error}",
                            target.overlay_path
                        ),
                    );
                }
            }
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
    let input_identity = if let Some((overlay_root, overlay_path)) = overlay.as_ref() {
        let identity =
            overlay_input_identity(&request, overlay_root, overlay_path, mirror_path.as_deref())?;
        if can_reuse_overlay(&request, overlay_root, overlay_path, &identity)? {
            result.source_exists = identity.source_exists;
            return Ok(result);
        }
        Some(identity)
    } else {
        None
    };

    if let Some((overlay_root, overlay_path)) = overlay.as_ref() {
        remove_overlay_reuse_manifest(overlay_root, overlay_path)?;
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

    if let (Some((overlay_root, overlay_path)), Some(input_identity)) =
        (overlay.as_ref(), input_identity.as_ref())
    {
        let materialized_fingerprint = materialized_overlay_fingerprint(
            &request,
            overlay_path,
            result.linked_count,
            result.copied_count,
        )?;
        write_overlay_reuse_manifest(
            overlay_root,
            overlay_path,
            &AgentRuntimeOverlayReuseManifest {
                schema_version: OVERLAY_REUSE_SCHEMA_VERSION,
                request_fingerprint: input_identity.fingerprint.clone(),
                materialized_fingerprint,
                source_exists: result.source_exists,
                linked_count: result.linked_count,
                copied_count: result.copied_count,
            },
        )?;
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

fn overlay_input_identity(
    request: &AgentRuntimeOverlayRequest,
    overlay_root: &Path,
    overlay_path: &Path,
    mirror_path: Option<&Path>,
) -> Result<AgentRuntimeOverlayInputIdentity, String> {
    let mut hasher = Sha256::new();
    hash_u64(&mut hasher, OVERLAY_REUSE_SCHEMA_VERSION);
    hash_text(&mut hasher, "overlay-input-v1");
    hash_path(&mut hasher, overlay_root);
    hash_path(&mut hasher, overlay_path);
    match mirror_path {
        Some(path) => {
            hash_text(&mut hasher, "mirror-present");
            hash_path(&mut hasher, path);
        }
        None => hash_text(&mut hasher, "mirror-absent"),
    }

    match request.managed_subdirectory.as_deref() {
        Some(value) => {
            hash_text(&mut hasher, "managed-subdirectory");
            hash_text(&mut hasher, value);
        }
        None => hash_text(&mut hasher, "managed-subdirectory-absent"),
    }
    let mut managed_file_names = request.managed_file_names.iter().collect::<Vec<_>>();
    managed_file_names.sort();
    hash_u64(&mut hasher, managed_file_names.len() as u64);
    for name in managed_file_names {
        hash_text(&mut hasher, name);
    }

    hash_u64(&mut hasher, request.managed_files.len() as u64);
    for managed_file in &request.managed_files {
        let allowed_root = normalize_absolute(Path::new(&managed_file.allowed_root))?;
        let path = normalize_absolute(Path::new(&managed_file.path))?;
        ensure_strict_descendant(&path, &allowed_root, "managed file path")?;
        hash_path(&mut hasher, &allowed_root);
        hash_path(&mut hasher, &path);
        hash_bytes(&mut hasher, managed_file.content.as_bytes());
        hash_text(
            &mut hasher,
            match managed_file.write_mode {
                AgentRuntimeOverlayWriteMode::Replace => "replace",
                AgentRuntimeOverlayWriteMode::CreateIfMissing => "create-if-missing",
            },
        );
        hash_bool(&mut hasher, managed_file.executable);
    }

    let source_exists = match request.source_path.as_deref() {
        Some(source) => {
            let source = normalize_absolute(Path::new(source))?;
            hash_text(&mut hasher, "source-present-in-request");
            hash_path(&mut hasher, &source);
            hash_mirrored_source(
                &mut hasher,
                &source,
                request.managed_subdirectory.as_deref(),
                &request.managed_file_names,
            )?
        }
        None => {
            hash_text(&mut hasher, "source-absent-in-request");
            false
        }
    };

    Ok(AgentRuntimeOverlayInputIdentity {
        fingerprint: finalize_sha256_hex(hasher),
        source_exists,
    })
}

fn hash_mirrored_source(
    hasher: &mut Sha256,
    source_path: &Path,
    managed_subdirectory: Option<&str>,
    managed_file_names: &[String],
) -> Result<bool, String> {
    let exists = entity_exists(source_path)?;
    hash_bool(hasher, exists);
    if !exists {
        return Ok(false);
    }

    let metadata = fs::metadata(source_path)
        .map_err(|error| format!("failed to stat source {}: {error}", source_path.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "overlay source is not a directory: {}",
            source_path.display()
        ));
    }

    for entry in read_directory_entries(source_path)? {
        let entry_name = entry.file_name();
        let entry_path = entry.path();
        if managed_subdirectory.is_some_and(|managed| entry_name == managed)
            && fs::metadata(&entry_path)
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false)
        {
            hash_text(hasher, "managed-source-directory");
            hash_os_string(hasher, &entry_name);
            for child in read_directory_entries(&entry_path)? {
                let child_name = child.file_name();
                let child_name_lossy = child_name.to_string_lossy();
                if managed_file_names
                    .iter()
                    .any(|name| child_name_lossy == name.as_str())
                {
                    continue;
                }
                hash_os_string(hasher, &child_name);
                hash_filesystem_entity(hasher, &child.path())?;
            }
            continue;
        }
        hash_os_string(hasher, &entry_name);
        hash_filesystem_entity(hasher, &entry_path)?;
    }
    Ok(true)
}

fn can_reuse_overlay(
    request: &AgentRuntimeOverlayRequest,
    overlay_root: &Path,
    overlay_path: &Path,
    input_identity: &AgentRuntimeOverlayInputIdentity,
) -> Result<bool, String> {
    let Some(manifest) = read_overlay_reuse_manifest(overlay_root, overlay_path)? else {
        return Ok(false);
    };
    if manifest.schema_version != OVERLAY_REUSE_SCHEMA_VERSION
        || manifest.request_fingerprint != input_identity.fingerprint
        || manifest.source_exists != input_identity.source_exists
    {
        return Ok(false);
    }

    let current_fingerprint = match materialized_overlay_fingerprint(
        request,
        overlay_path,
        manifest.linked_count,
        manifest.copied_count,
    ) {
        Ok(fingerprint) => fingerprint,
        Err(_) => return Ok(false),
    };
    Ok(current_fingerprint == manifest.materialized_fingerprint)
}

fn materialized_overlay_fingerprint(
    request: &AgentRuntimeOverlayRequest,
    overlay_path: &Path,
    linked_count: u64,
    copied_count: u64,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "overlay-materialized-v1");
    hash_u64(&mut hasher, linked_count);
    hash_u64(&mut hasher, copied_count);
    hash_filesystem_entity(&mut hasher, overlay_path)?;

    hash_u64(&mut hasher, request.managed_files.len() as u64);
    for managed_file in &request.managed_files {
        let allowed_root = normalize_absolute(Path::new(&managed_file.allowed_root))?;
        let path = normalize_absolute(Path::new(&managed_file.path))?;
        ensure_strict_descendant(&path, &allowed_root, "managed file path")?;
        hash_path(&mut hasher, &path);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                hash_text(&mut hasher, "managed-materialized-present");
                hash_filesystem_entity(&mut hasher, &path)?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                hash_text(&mut hasher, "managed-materialized-missing");
            }
            Err(error) => {
                return Err(format!("failed to stat {}: {error}", path.display()));
            }
        }
    }
    Ok(finalize_sha256_hex(hasher))
}

fn hash_filesystem_entity(hasher: &mut Sha256, path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))?;
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        hash_text(hasher, "symlink");
        let target = fs::read_link(path)
            .map_err(|error| format!("failed to read link {}: {error}", path.display()))?;
        hash_path(hasher, &target);
        return Ok(());
    }
    if file_type.is_dir() {
        hash_text(hasher, "directory");
        for entry in read_directory_entries(path)? {
            hash_os_string(hasher, &entry.file_name());
            hash_filesystem_entity(hasher, &entry.path())?;
        }
        return Ok(());
    }
    if file_type.is_file() {
        hash_text(hasher, "file");
        hash_u64(hasher, metadata.len());
        hash_bool(hasher, metadata.permissions().readonly());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash_u64(hasher, metadata.permissions().mode() as u64);
        }
        let mut file = File::open(path)
            .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            hash_bytes(hasher, &buffer[..read]);
        }
        hash_u64(hasher, 0);
        return Ok(());
    }

    Err(format!(
        "unsupported filesystem entity in overlay fingerprint: {}",
        path.display()
    ))
}

fn overlay_reuse_manifest_path(
    overlay_root: &Path,
    overlay_path: &Path,
) -> Result<PathBuf, String> {
    let root = normalize_absolute(overlay_root)?;
    let target = normalize_absolute(overlay_path)?;
    ensure_strict_descendant(&target, &root, "overlay path")?;
    let relative = target.strip_prefix(&root).map_err(|_| {
        format!(
            "overlay path {} is not under {}",
            target.display(),
            root.display()
        )
    })?;
    let mut key_hasher = Sha256::new();
    hash_path(&mut key_hasher, relative);
    let key = finalize_sha256_hex(key_hasher);
    Ok(root
        .join(OVERLAY_REUSE_DIRECTORY)
        .join(format!("{key}.json")))
}

fn read_overlay_reuse_manifest(
    overlay_root: &Path,
    overlay_path: &Path,
) -> Result<Option<AgentRuntimeOverlayReuseManifest>, String> {
    let path = overlay_reuse_manifest_path(overlay_root, overlay_path)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    match serde_json::from_slice(&bytes) {
        Ok(manifest) => Ok(Some(manifest)),
        Err(_) => Ok(None),
    }
}

fn write_overlay_reuse_manifest(
    overlay_root: &Path,
    overlay_path: &Path,
    manifest: &AgentRuntimeOverlayReuseManifest,
) -> Result<(), String> {
    let path = overlay_reuse_manifest_path(overlay_root, overlay_path)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("reuse manifest has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    let tmp_path = unique_temp_path(parent);
    let mut payload = serde_json::to_vec(manifest)
        .map_err(|error| format!("failed to encode overlay reuse manifest: {error}"))?;
    payload.push(b'\n');
    fs::write(&tmp_path, payload)
        .map_err(|error| format!("failed to write {}: {error}", tmp_path.display()))?;
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!("failed to replace {}: {error}", path.display()));
        }
    }
    fs::rename(&tmp_path, &path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        format!(
            "failed to rename {} to {}: {error}",
            tmp_path.display(),
            path.display()
        )
    })
}

fn remove_overlay_reuse_manifest(overlay_root: &Path, overlay_path: &Path) -> Result<(), String> {
    let path = overlay_reuse_manifest_path(overlay_root, overlay_path)?;
    match fs::remove_file(&path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
    }
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hash_bytes(hasher, value.as_bytes());
}

fn hash_path(hasher: &mut Sha256, path: &Path) {
    hash_bytes(hasher, path.to_string_lossy().as_bytes());
}

fn hash_os_string(hasher: &mut Sha256, value: &OsString) {
    hash_bytes(hasher, value.to_string_lossy().as_bytes());
}

fn hash_bytes(hasher: &mut Sha256, value: &[u8]) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value);
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_le_bytes());
}

fn hash_bool(hasher: &mut Sha256, value: bool) {
    hasher.update([u8::from(value)]);
}

fn finalize_sha256_hex(hasher: Sha256) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
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

    fn only_reuse_manifest(root: &Path) -> PathBuf {
        let manifest_root = root.join("overlays/.alera-overlay-reuse");
        let entries = fs::read_dir(&manifest_root)
            .unwrap_or_else(|error| {
                panic!(
                    "expected reuse manifest directory {}: {error}",
                    manifest_root.display()
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1, "expected exactly one reuse manifest");
        entries[0].path()
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
        assert!(!result.warnings.is_empty());
        assert!(result
            .warnings
            .iter()
            .all(|warning| warning.contains("copied instead")));
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
        let changed =
            prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
                .unwrap();
        assert!(changed.removed_count > 0);
        assert_eq!(changed.written_count, 1);
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

        let result =
            prepare_agent_runtime_overlay_with_linker(request.clone(), |source, target| {
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

        let repeated = prepare_agent_runtime_overlay_with_linker(request, |_, _| {
            panic!("unchanged linked overlay must not invoke the linker")
        })
        .unwrap();
        assert!(repeated.source_exists);
        assert_eq!(repeated.removed_count, 0);
        assert_eq!(repeated.written_count, 0);
        assert_eq!(repeated.linked_count, 0);
        assert_eq!(repeated.copied_count, 0);
        assert!(repeated.warnings.is_empty());
    }

    #[test]
    fn actual_platform_link_or_copy_reconciles_repeated_unchanged_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("plugins")).unwrap();
        fs::write(source.join("settings.json"), "settings-v1").unwrap();
        fs::write(source.join("plugins/user.js"), "user-plugin").unwrap();
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        let first = prepare_agent_runtime_overlay(request.clone()).unwrap();
        assert!(first.source_exists);
        assert_eq!(first.written_count, 1);
        assert!(first.linked_count + first.copied_count >= 2);
        if first.copied_count > 0 {
            assert!(!first.warnings.is_empty());
        }
        assert_eq!(
            fs::read_to_string(overlay.join("settings.json")).unwrap(),
            "settings-v1"
        );
        assert_eq!(
            fs::read_to_string(overlay.join("plugins/user.js")).unwrap(),
            "user-plugin"
        );

        let repeated = prepare_agent_runtime_overlay(request).unwrap();
        assert!(repeated.source_exists);
        assert_eq!(repeated.removed_count, 0);
        assert_eq!(repeated.written_count, 0);
        assert_eq!(repeated.linked_count, 0);
        assert_eq!(repeated.copied_count, 0);
        assert!(repeated.warnings.is_empty());
        assert_eq!(
            fs::read_to_string(overlay.join("settings.json")).unwrap(),
            "settings-v1"
        );
    }

    #[test]
    fn forced_copy_reuses_an_exactly_unchanged_overlay_without_invoking_the_linker() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("plugins")).unwrap();
        fs::write(source.join("settings.json"), "settings-v1").unwrap();
        fs::write(source.join("plugins/user.js"), "user-plugin").unwrap();
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        let first = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        assert!(first.copied_count >= 2);
        assert!(overlay.join(".alera-copied-resources").is_dir());

        let repeated = prepare_agent_runtime_overlay_with_linker(request, |_, _| {
            panic!("unchanged copied overlay must not invoke the linker")
        })
        .unwrap();
        assert!(repeated.source_exists);
        assert_eq!(repeated.removed_count, 0);
        assert_eq!(repeated.written_count, 0);
        assert_eq!(repeated.linked_count, 0);
        assert_eq!(repeated.copied_count, 0);
        assert!(repeated.warnings.is_empty());
    }

    #[test]
    fn source_content_and_shape_changes_invalidate_reuse() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("one.txt"), "one-v1").unwrap();
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();

        fs::write(source.join("one.txt"), "one-v2").unwrap();
        let content_changed = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        assert!(content_changed.removed_count > 0);
        assert_eq!(
            fs::read_to_string(overlay.join("one.txt")).unwrap(),
            "one-v2"
        );

        fs::write(source.join("two.txt"), "two").unwrap();
        let added = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        assert!(added.removed_count > 0);
        assert!(overlay.join("two.txt").is_file());

        fs::rename(source.join("two.txt"), source.join("renamed.txt")).unwrap();
        let renamed = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        assert!(renamed.removed_count > 0);
        assert!(!overlay.join("two.txt").exists());
        assert!(overlay.join("renamed.txt").is_file());

        fs::remove_file(source.join("renamed.txt")).unwrap();
        let removed =
            prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
                .unwrap();
        assert!(removed.removed_count > 0);
        assert!(!overlay.join("renamed.txt").exists());
    }

    #[test]
    fn changed_generated_wrapper_content_invalidates_reuse() {
        let root = tempfile::tempdir().unwrap();
        let mut request = request_for(root.path(), None);
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());
        request.managed_files.push(AgentRuntimeOverlayManagedFile {
            path: overlay.join("bin/agent.cmd").to_string_lossy().into_owned(),
            allowed_root: overlay.to_string_lossy().into_owned(),
            content: "@echo wrapper-v1\r\n".into(),
            write_mode: AgentRuntimeOverlayWriteMode::Replace,
            executable: true,
        });

        prepare_agent_runtime_overlay(request.clone()).unwrap();
        let unchanged = prepare_agent_runtime_overlay(request.clone()).unwrap();
        assert_eq!(unchanged.removed_count, 0);
        assert_eq!(unchanged.written_count, 0);

        request.managed_files[1].content = "@echo wrapper-v2\r\n".into();
        let changed = prepare_agent_runtime_overlay(request).unwrap();
        assert!(changed.removed_count > 0);
        assert_eq!(changed.written_count, 2);
        assert_eq!(
            fs::read_to_string(overlay.join("bin/agent.cmd")).unwrap(),
            "@echo wrapper-v2\r\n"
        );
    }

    #[test]
    fn removed_or_corrupted_overlay_target_invalidates_reuse() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("settings.json"), "settings-v1").unwrap();
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        fs::write(overlay.join("settings.json"), "corrupted").unwrap();
        let repaired = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        assert!(repaired.removed_count > 0);
        assert_eq!(
            fs::read_to_string(overlay.join("settings.json")).unwrap(),
            "settings-v1"
        );

        fs::remove_dir_all(&overlay).unwrap();
        let rebuilt =
            prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
                .unwrap();
        assert!(rebuilt.copied_count > 0);
        assert_eq!(
            fs::read_to_string(overlay.join("settings.json")).unwrap(),
            "settings-v1"
        );
    }

    #[test]
    fn copied_resource_marker_and_manifest_schema_are_part_of_reuse_validity() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("settings.json"), "settings-v1").unwrap();
        let request = request_for(root.path(), Some(&source));
        let overlay = PathBuf::from(request.overlay_path.clone().unwrap());

        prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        fs::remove_dir_all(overlay.join(".alera-copied-resources")).unwrap();
        let marker_rebuilt = prepare_agent_runtime_overlay_with_linker(request.clone(), |_, _| {
            Err("links disabled".into())
        })
        .unwrap();
        assert!(marker_rebuilt.removed_count > 0);
        assert!(marker_rebuilt.copied_count > 0);

        let manifest = only_reuse_manifest(root.path());
        let mut payload: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        payload["schemaVersion"] = serde_json::Value::from(0);
        fs::write(
            &manifest,
            format!("{}\n", serde_json::to_string(&payload).unwrap()),
        )
        .unwrap();

        let schema_rebuilt =
            prepare_agent_runtime_overlay_with_linker(request, |_, _| Err("links disabled".into()))
                .unwrap();
        assert!(schema_rebuilt.removed_count > 0);
        assert!(schema_rebuilt.copied_count > 0);
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

    #[derive(Clone, Copy, Debug)]
    enum BenchmarkLinkMode {
        LinkSuccess,
        CopyFallback,
    }

    impl BenchmarkLinkMode {
        fn label(self) -> &'static str {
            match self {
                Self::LinkSuccess => "link-success",
                Self::CopyFallback => "copy-fallback",
            }
        }
    }

    fn prepare_benchmark_overlay(
        request: AgentRuntimeOverlayRequest,
        mode: BenchmarkLinkMode,
    ) -> AgentRuntimeOverlayResult {
        match mode {
            BenchmarkLinkMode::LinkSuccess => {
                prepare_agent_runtime_overlay_with_linker(request, |source, target| {
                    fs::hard_link(source, target).map_err(|error| error.to_string())
                })
                .unwrap()
            }
            BenchmarkLinkMode::CopyFallback => {
                prepare_agent_runtime_overlay_with_linker(request, |_, _| {
                    Err("benchmark forces copy fallback".into())
                })
                .unwrap()
            }
        }
    }

    fn create_benchmark_source(source: &Path, file_count: usize) {
        fs::create_dir_all(source).unwrap();
        for index in 0..file_count {
            fs::write(
                source.join(format!("file-{index:04}.txt")),
                b"0123456789abcdef",
            )
            .unwrap();
        }
    }

    fn assert_benchmark_result(
        result: &AgentRuntimeOverlayResult,
        mode: BenchmarkLinkMode,
        file_count: usize,
    ) {
        assert!(result.source_exists);
        assert_eq!(result.written_count, 1);
        match mode {
            BenchmarkLinkMode::LinkSuccess => {
                assert_eq!(result.linked_count, file_count as u64);
                assert_eq!(result.copied_count, 0);
                assert!(result.warnings.is_empty());
            }
            BenchmarkLinkMode::CopyFallback => {
                assert_eq!(result.linked_count, 0);
                assert_eq!(result.copied_count, file_count as u64);
                assert!(!result.warnings.is_empty());
            }
        }
    }

    fn assert_benchmark_reuse_result(result: &AgentRuntimeOverlayResult) {
        assert!(result.source_exists);
        assert_eq!(result.removed_count, 0);
        assert_eq!(result.written_count, 0);
        assert_eq!(result.linked_count, 0);
        assert_eq!(result.copied_count, 0);
        assert!(result.warnings.is_empty());
    }

    #[test]
    #[ignore = "manual five-sample overlay production benchmark"]
    fn benchmark_overlay_production_matrix() {
        for (size, file_count) in [("small", 20usize), ("medium", 500), ("large", 2_000)] {
            for mode in [
                BenchmarkLinkMode::LinkSuccess,
                BenchmarkLinkMode::CopyFallback,
            ] {
                let mut first_samples = Vec::with_capacity(5);
                let mut repeated_samples = Vec::with_capacity(5);

                for sample in 1..=5 {
                    let root = tempfile::tempdir().unwrap();
                    let source = root.path().join("source");
                    create_benchmark_source(&source, file_count);
                    let request = request_for(root.path(), Some(&source));

                    let first_started = Instant::now();
                    let first = prepare_benchmark_overlay(request.clone(), mode);
                    let first_us = first_started.elapsed().as_micros();
                    assert_benchmark_result(&first, mode, file_count);

                    let repeated_started = Instant::now();
                    let repeated = prepare_benchmark_overlay(request, mode);
                    let repeated_us = repeated_started.elapsed().as_micros();
                    assert_benchmark_reuse_result(&repeated);

                    eprintln!(
                        "overlay_a3 size={size} files={file_count} mode={} sample={sample} first_us={first_us} repeated_unchanged_us={repeated_us} first_removed={} repeated_removed={} warnings={}",
                        mode.label(),
                        first.removed_count,
                        repeated.removed_count,
                        repeated.warnings.len(),
                    );
                    first_samples.push(first_us);
                    repeated_samples.push(repeated_us);
                }

                first_samples.sort_unstable();
                repeated_samples.sort_unstable();
                eprintln!(
                    "overlay_a3_summary size={size} files={file_count} mode={} first_median_us={} repeated_unchanged_median_us={} first_samples={first_samples:?} repeated_samples={repeated_samples:?}",
                    mode.label(),
                    first_samples[first_samples.len() / 2],
                    repeated_samples[repeated_samples.len() / 2],
                );
            }
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
