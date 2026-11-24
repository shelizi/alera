//! Conservative account-to-account native transcript mirroring.
//! A provider may refuse to resume another account's mirrored transcript.
//! This only mirrors session files; it never copies credentials or settings.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::agent_history_snapshots::{preserve_version, ArchiveResult};
#[path = "agent_history_sync_io.rs"]
mod sync_io;
use alera_core::runtime::{
    RuntimeAgentHistorySyncMode, RuntimeAgentHistorySyncSettings, RuntimeAgentQuotaSettings,
    RuntimeStore,
};
use sha2::{Digest, Sha256};
use sync_io::inspect_prefix;
#[path = "agent_history_sync_walk.rs"]
mod sync_walk;
use uuid::Uuid;

const INTERVAL: Duration = Duration::from_secs(3 * 60);
const QUIET_PERIOD: Duration = Duration::from_secs(2 * 60);
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES_PER_PASS: usize = 120;
const MAX_DEPTH: usize = 12;

#[derive(Debug, Clone)]
struct HistoryAccount {
    provider: &'static str,
    account_id: String,
    history_root: PathBuf,
}

/// Reuse account-separated native history roots for an explicit local search.
/// Never infer additional roots from untrusted transcript content.
pub(crate) async fn search_roots(
    settings: &RuntimeAgentQuotaSettings,
) -> Vec<crate::agent_session_history_scan::ProviderHistoryRoot> {
    resolve_accounts(settings)
        .await
        .into_iter()
        .filter(|account| account.history_root.is_absolute())
        .map(
            |account| crate::agent_session_history_scan::ProviderHistoryRoot {
                provider: account.provider,
                account_id: account.account_id,
                path: account.history_root,
            },
        )
        .collect()
}

#[derive(Default, Debug, PartialEq, Eq)]
struct MirrorStats {
    copied: usize,
    skipped: usize,
    conflicts: usize,
    stale: usize,
    archived: usize,
    archive_capacity: usize,
    archive_errors: usize,
    #[cfg(test)]
    directories_read: usize,
}

pub(crate) fn spawn(
    runtime_store: RuntimeStore,
    snapshot_root: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let Ok(settings) = runtime_store.agent_quota_settings().await else {
                continue;
            };
            if settings.history_sync.mode == RuntimeAgentHistorySyncMode::Separate {
                continue;
            }
            let accounts = resolve_accounts(&settings).await;
            if accounts.len() < 2 {
                continue;
            }
            let policy = settings.history_sync;
            let snapshot_root = snapshot_root.clone();
            match tokio::task::spawn_blocking(move || {
                mirror_accounts_with_snapshots(&policy, &accounts, QUIET_PERIOD, Some(&snapshot_root))
            })
            .await
            {
                Ok(stats) if stats.conflicts > 0 || stats.stale > 0 || stats.archive_errors > 0 || stats.archive_capacity > 0 => tracing::warn!(
                    conflicts = stats.conflicts,
                    stale = stats.stale,
                    archived = stats.archived,
                    archive_capacity = stats.archive_capacity,
                    archive_errors = stats.archive_errors,
                    "agent history sync preserved native files and archived stale transcript versions"
                ),
                Err(_) => tracing::warn!("agent history sync worker failed"),
                _ => {}
            }
        }
    })
}

async fn resolve_accounts(settings: &RuntimeAgentQuotaSettings) -> Vec<HistoryAccount> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let codex_home = crate::login_shell_environment::login_shell_variable("CODEX_HOME")
        .await
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let ccs_home = crate::login_shell_environment::login_shell_variable("CCS_DIR")
        .await
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".ccs"));
    let mut accounts = vec![
        HistoryAccount {
            provider: "codex",
            account_id: "default".into(),
            history_root: codex_home.join("sessions"),
        },
        HistoryAccount {
            provider: "claude",
            account_id: "default".into(),
            history_root: home.join(".claude").join("projects"),
        },
    ];
    for profile in &settings.codex_profiles {
        accounts.push(HistoryAccount {
            provider: "codex",
            account_id: profile.profile.clone(),
            history_root: Path::new(&profile.profile).join("sessions"),
        });
    }
    for profile in &settings.claude_profiles {
        let config_dir = Path::new(&profile.profile);
        let config_dir = if config_dir.is_absolute() {
            config_dir.to_path_buf()
        } else {
            ccs_home.join("instances").join(&profile.profile)
        };
        accounts.push(HistoryAccount {
            provider: "claude",
            account_id: profile.profile.clone(),
            history_root: config_dir.join("projects"),
        });
    }
    // AGY storage and provider-backed resume must be separately validated.
    // Never guess a Gemini directory and accidentally copy its credentials.
    accounts
}
#[cfg(test)]
fn mirror_accounts(
    policy: &RuntimeAgentHistorySyncSettings,
    accounts: &[HistoryAccount],
    quiet: Duration,
) -> MirrorStats {
    mirror_accounts_with_snapshots(policy, accounts, quiet, None)
}

fn mirror_accounts_with_snapshots(
    policy: &RuntimeAgentHistorySyncSettings,
    accounts: &[HistoryAccount],
    quiet: Duration,
    snapshot_root: Option<&Path>,
) -> MirrorStats {
    let mut stats = MirrorStats::default();
    let mut budget = MAX_FILES_PER_PASS;
    for source in accounts {
        if budget == 0 {
            break;
        }
        // Resolve the source only if a same-provider, policy-authorized
        // destination exists. Separate mode and single-account providers
        // must not touch the filesystem.
        let mut targets = accounts
            .iter()
            .filter(|target| {
                source.provider == target.provider
                    && policy.should_sync(source.provider, &source.account_id, &target.account_id)
            })
            .peekable();
        if targets.peek().is_none() || !real_directory(&source.history_root) {
            continue;
        }
        let Ok(source_root) = fs::canonicalize(&source.history_root) else {
            continue;
        };
        let targets: Vec<_> = targets
            .filter(|target| {
                if !real_directory(&target.history_root) {
                    return false;
                }
                fs::canonicalize(&target.history_root).is_ok_and(|root| {
                    // No recursive copying between overlapping roots.
                    !source_root.starts_with(&root) && !root.starts_with(&source_root)
                })
            })
            .collect();
        if targets.is_empty() {
            continue;
        }

        // One streaming directory traversal per source, not per directed
        // account pair. Process destinations in their configured order for
        // each file; no unbounded inventory or cross-pass cache of paths.
        let walked =
            sync_walk::walk_transcripts(&source.history_root, MAX_DEPTH, |path, relative| {
                for target in &targets {
                    if budget == 0 {
                        return false;
                    }
                    mirror_transcript(
                        source,
                        target,
                        path,
                        relative,
                        quiet,
                        snapshot_root,
                        &mut budget,
                        &mut stats,
                    );
                }
                budget > 0
            });
        stats.skipped += walked.skipped.saturating_mul(targets.len());
        #[cfg(test)]
        {
            stats.directories_read += walked.directories_read;
        }
    }
    stats
}

#[allow(clippy::too_many_arguments)]
fn mirror_transcript(
    source: &HistoryAccount,
    target: &HistoryAccount,
    path: &Path,
    child: &Path,
    quiet: Duration,
    snapshot_root: Option<&Path>,
    budget: &mut usize,
    stats: &mut MirrorStats,
) {
    // Previously mirrored files must not consume the copy budget.
    // Otherwise, large histories starve newer sessions indefinitely.
    let target_file = target.history_root.join(child);
    if let Ok(existing) = fs::symlink_metadata(&target_file) {
        if let Ok(original) = fs::symlink_metadata(path) {
            if !existing.file_type().is_file() || !original.file_type().is_file() {
                stats.conflicts += 1;
            } else if existing.len() < original.len() {
                let capture = snapshot_root.is_some()
                    && original
                        .modified()
                        .ok()
                        .and_then(|time| SystemTime::now().duration_since(time).ok())
                        .is_some_and(|age| age >= quiet);
                match inspect_prefix(
                    &target_file,
                    &existing,
                    path,
                    &original,
                    MAX_FILE_BYTES,
                    capture,
                ) {
                    Ok(Some(verified)) => {
                        stats.stale += 1;
                        if let Some(root) = snapshot_root {
                            let result = verified.snapshot.and_then(|bytes| {
                                archive_stale_copy(root, source, target, child, bytes)
                            });
                            match result {
                                Ok(Some(ArchiveResult::Created)) => stats.archived += 1,
                                Ok(Some(ArchiveResult::AlreadyPresent)) | Ok(None) => {}
                                Ok(Some(ArchiveResult::CapacityReached)) => {
                                    stats.archive_capacity += 1
                                }
                                Err(_) => stats.archive_errors += 1,
                            }
                        }
                    }
                    _ => stats.conflicts += 1,
                }
            } else if existing.len() > original.len()
                && !matches!(
                    inspect_prefix(
                        path,
                        &original,
                        &target_file,
                        &existing,
                        MAX_FILE_BYTES,
                        false,
                    ),
                    Ok(Some(_))
                )
            {
                stats.conflicts += 1;
            }
        }
        return;
    }
    *budget -= 1;
    match mirror_file(path, &target.history_root, child, quiet) {
        CopyOutcome::Copied => stats.copied += 1,
        CopyOutcome::Exists => stats.conflicts += 1,
        CopyOutcome::Skipped => stats.skipped += 1,
    }
}

/// Preserve the newest complete, quiet source in Host-private version storage.
/// The provider's target JSONL is only read. Never change native session files
/// because an unrelated CLI may still be holding a resume handle to that path.
fn archive_stale_copy(
    archive_root: &Path,
    source: &HistoryAccount,
    target: &HistoryAccount,
    relative: &Path,
    snapshot: Option<Vec<u8>>,
) -> io::Result<Option<ArchiveResult>> {
    if archive_root.starts_with(&source.history_root)
        || archive_root.starts_with(&target.history_root)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Archive overlaps provider history",
        ));
    }

    let Some(full) = snapshot else {
        return Ok(None);
    };

    // Hash the complete scoped identity to avoid filesystem path traversal
    // and keep private account directory names out of snapshot filenames.
    let mut identity = Sha256::new();
    let relative_path = relative.to_string_lossy();
    for part in [
        source.provider.as_bytes(),
        source.account_id.as_bytes(),
        target.account_id.as_bytes(),
        relative_path.as_bytes(),
    ] {
        identity.update((part.len() as u32).to_le_bytes());
        identity.update(part);
    }
    let identity = hex::encode(identity.finalize());
    let version = hex::encode(Sha256::digest(&full));
    let metadata = serde_json::to_vec(&serde_json::json!({
        "provider": source.provider,
        "sourceAccount": source.account_id,
        "targetAccount": target.account_id,
        "relativePath": relative.to_string_lossy(),
        "format": "native-jsonl",
        "note": "Alera private snapshot; never import into another Agent login without verification"
    }))
    .map_err(io::Error::other)?;
    preserve_version(archive_root, &identity, &version, &full, &metadata).map(Some)
}

#[derive(PartialEq, Eq)]
enum CopyOutcome {
    Copied,
    Exists,
    Skipped,
}

fn mirror_file(source: &Path, root: &Path, relative: &Path, quiet: Duration) -> CopyOutcome {
    let Ok(before) = fs::symlink_metadata(source) else {
        return CopyOutcome::Skipped;
    };
    if !before.file_type().is_file() || before.len() == 0 || before.len() > MAX_FILE_BYTES {
        return CopyOutcome::Skipped;
    }
    if before
        .modified()
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .is_none_or(|age| age < quiet)
    {
        return CopyOutcome::Skipped;
    }
    let Some(parent) = safe_target_parent(root, relative) else {
        return CopyOutcome::Skipped;
    };
    let Some(file_name) = relative.file_name() else {
        return CopyOutcome::Skipped;
    };
    let target = parent.join(file_name);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        // Never overwrite any existing native transcript: another account
        // could be resuming it even when it is currently quiet.
        if meta.file_type().is_file() && meta.len() == before.len() {
            if let (Ok(src), Ok(dst)) = (fs::read(source), fs::read(&target)) {
                if src == dst {
                    return CopyOutcome::Skipped;
                }
            }
        }
        return CopyOutcome::Exists;
    }
    let Ok(bytes) = fs::read(source) else {
        return CopyOutcome::Skipped;
    };
    let Ok(after) = fs::symlink_metadata(source) else {
        return CopyOutcome::Skipped;
    };
    if before.len() != bytes.len() as u64
        || after.len() != before.len()
        || after.modified().ok() != before.modified().ok()
    {
        return CopyOutcome::Skipped;
    }
    let temp = parent.join(format!(".alera-history-{}.tmp", Uuid::new_v4()));
    let written = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
        return CopyOutcome::Skipped;
    }
    // hard_link is an atomic no-clobber create, unlike a Unix rename.
    let result = fs::hard_link(&temp, &target);
    let _ = fs::remove_file(&temp);
    match result {
        Ok(()) => CopyOutcome::Copied,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => CopyOutcome::Exists,
        Err(_) => CopyOutcome::Skipped,
    }
}

fn real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

fn safe_target_parent(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.parent()?.components() {
        let Component::Normal(segment) = component else {
            return None;
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if fs::create_dir(&current).is_err() || !real_directory(&current) {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(current)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn account(provider: &'static str, id: &str, root: PathBuf) -> HistoryAccount {
        HistoryAccount {
            provider,
            account_id: id.into(),
            history_root: root,
        }
    }

    fn fixture() -> (tempfile::TempDir, Vec<HistoryAccount>) {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a").join("sessions");
        let b = dir.path().join("b").join("sessions");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::create_dir_all(a.join("2026").join("11")).unwrap();
        fs::write(a.join("2026/11/first.jsonl"), "{\"type\":\"message\"}\n").unwrap();
        fs::write(dir.path().join("a").join("auth.json"), "secret").unwrap();
        (
            dir,
            vec![account("codex", "default", a), account("codex", "b", b)],
        )
    }

    fn sharing_all() -> RuntimeAgentHistorySyncSettings {
        RuntimeAgentHistorySyncSettings {
            mode: RuntimeAgentHistorySyncMode::All,
            ..Default::default()
        }
    }

    #[test]
    fn default_separate_does_not_copy_or_archive_any_account_history() {
        let (directory, accounts) = fixture();
        let archive = directory.path().join("host-snapshots");
        let result = mirror_accounts_with_snapshots(
            &RuntimeAgentHistorySyncSettings::default(),
            &accounts,
            Duration::ZERO,
            Some(&archive),
        );
        assert_eq!(result.copied, 0);
        assert_eq!(result.archived, 0);
        assert!(!accounts[1]
            .history_root
            .join("2026/11/first.jsonl")
            .exists());
        assert!(!archive.exists());
    }

    #[test]
    fn explicitly_enabled_all_mirrors_transcripts_and_preserves_credentials() {
        let (dir, accounts) = fixture();
        let stats = mirror_accounts(&sharing_all(), &accounts, Duration::ZERO);
        assert_eq!(stats.copied, 1);
        assert_eq!(
            fs::read_to_string(accounts[1].history_root.join("2026/11/first.jsonl")).unwrap(),
            "{\"type\":\"message\"}\n"
        );
        assert!(!dir.path().join("b").join("auth.json").exists());
        assert!(dir.path().join("a").join("auth.json").exists());
    }

    #[test]
    fn separate_disables_history_copy() {
        let (_, accounts) = fixture();
        let policy = RuntimeAgentHistorySyncSettings {
            mode: RuntimeAgentHistorySyncMode::Separate,
            ..Default::default()
        };
        assert_eq!(
            mirror_accounts(&policy, &accounts, Duration::ZERO).copied,
            0
        );
        assert!(!accounts[1]
            .history_root
            .join("2026/11/first.jsonl")
            .exists());
    }

    #[test]
    fn group_sync_requires_two_matching_memberships() {
        let (_dir, accounts) = fixture();
        let mut policy = RuntimeAgentHistorySyncSettings {
            mode: RuntimeAgentHistorySyncMode::Groups,
            account_groups: BTreeMap::from([
                ("codex:default".into(), "work".into()),
                ("codex:b".into(), "private".into()),
            ]),
        };
        assert_eq!(
            mirror_accounts(&policy, &accounts, Duration::ZERO).copied,
            0
        );
        policy
            .account_groups
            .insert("codex:b".into(), "work".into());
        assert_eq!(
            mirror_accounts(&policy, &accounts, Duration::ZERO).copied,
            1
        );
    }

    #[test]
    fn conflicting_destination_is_never_overwritten_or_removed() {
        let (_dir, accounts) = fixture();
        let target = accounts[1].history_root.join("2026/11/first.jsonl");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "other-account history").unwrap();
        let stats = mirror_accounts(&sharing_all(), &accounts, Duration::ZERO);
        assert_eq!(stats.copied, 0);
        assert!(stats.conflicts >= 1);
        assert_eq!(fs::read_to_string(target).unwrap(), "other-account history");
    }

    #[test]
    fn later_messages_are_reported_as_stale_and_never_overwrite_a_native_copy() {
        let (_dir, accounts) = fixture();
        let policy = sharing_all();
        assert_eq!(
            mirror_accounts(&policy, &accounts, Duration::ZERO).copied,
            1
        );
        let source = accounts[0].history_root.join("2026/11/first.jsonl");
        let target = accounts[1].history_root.join("2026/11/first.jsonl");
        let original = fs::read(&source).unwrap();
        let mut extended = original.clone();
        extended.extend_from_slice(b"{\"type\":\"followup\"}\n");
        fs::write(&source, extended).unwrap();
        let stats = mirror_accounts(&policy, &accounts, Duration::ZERO);
        assert_eq!(stats.copied, 0);
        assert_eq!(stats.stale, 1);
        assert_eq!(fs::read(target).unwrap(), original);
    }

    #[test]
    fn stale_native_copy_is_preserved_while_new_versions_are_archived_once() {
        let (temp, accounts) = fixture();
        let policy = sharing_all();
        let archive = temp.path().join("host-snapshots");
        assert_eq!(
            mirror_accounts(&policy, &accounts, Duration::ZERO).copied,
            1
        );
        let source = accounts[0].history_root.join("2026/11/first.jsonl");
        let target = accounts[1].history_root.join("2026/11/first.jsonl");
        let previous = fs::read(&target).unwrap();
        let mut new_version = previous.clone();
        new_version.extend_from_slice(b"{\"type\":\"followup\"}\n");
        fs::write(&source, &new_version).unwrap();

        let updated =
            mirror_accounts_with_snapshots(&policy, &accounts, Duration::ZERO, Some(&archive));
        assert_eq!(updated.stale, 1);
        assert_eq!(updated.archived, 1);
        assert_eq!(updated.archive_errors, 0);
        assert_eq!(
            fs::read(&target).unwrap(),
            previous,
            "never overwrite a provider session"
        );
        let bucket = fs::read_dir(&archive)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let snapshots = fs::read_dir(&bucket)
            .unwrap()
            .map(|v| v.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(snapshots.len(), 2, "one metadata file and one version");
        let archived = snapshots
            .into_iter()
            .find(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
            .unwrap();
        assert_eq!(fs::read(archived).unwrap(), new_version);

        let again =
            mirror_accounts_with_snapshots(&policy, &accounts, Duration::ZERO, Some(&archive));
        assert_eq!(again.stale, 1);
        assert_eq!(again.archived, 0, "content-addressing must dedupe");
    }

    #[test]
    fn never_archive_divergent_or_active_native_transcripts() {
        let (temp, accounts) = fixture();
        let archive = temp.path().join("host-snapshots");
        let policy = sharing_all();
        let source = accounts[0].history_root.join("2026/11/first.jsonl");
        let target = accounts[1].history_root.join("2026/11/first.jsonl");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"{\"type\":\"different\"}\n").unwrap();
        let divergent =
            mirror_accounts_with_snapshots(&policy, &accounts, Duration::ZERO, Some(&archive));
        assert_eq!(divergent.archived, 0);
        assert!(!archive.exists());
        fs::remove_file(&target).unwrap();
        assert_eq!(
            mirror_accounts(&policy, &accounts, Duration::ZERO).copied,
            1
        );
        let old = fs::read(&source).unwrap();
        let mut new_content = old.clone();
        new_content.extend_from_slice(b"{\"type\":\"more\"}\n");
        fs::write(&source, new_content).unwrap();
        let active = mirror_accounts_with_snapshots(
            &policy,
            &accounts,
            Duration::from_secs(3600),
            Some(&archive),
        );
        assert_eq!(active.archived, 0);
        assert!(!archive.exists());
    }

    #[test]
    fn fanout_walks_each_source_once_and_preserves_all_target_accounts() {
        let (temp, mut accounts) = fixture();
        for id in ["c", "d", "e"] {
            let root = temp.path().join(id).join("sessions");
            fs::create_dir_all(&root).unwrap();
            accounts.push(account("codex", id, root));
        }
        let policy = sharing_all();
        let first = mirror_accounts(&policy, &accounts, Duration::ZERO);
        assert_eq!(first.copied, 4);
        assert_eq!(
            first.directories_read, 15,
            "five sources with three directories each"
        );
        for account in &accounts {
            assert_eq!(
                fs::read(account.history_root.join("2026/11/first.jsonl")).unwrap(),
                b"{\"type\":\"message\"}\n"
            );
        }
        let second = mirror_accounts(&policy, &accounts, Duration::ZERO);
        assert_eq!(second.copied, 0);
        assert_eq!(second.directories_read, 15);
        let separate = mirror_accounts(
            &RuntimeAgentHistorySyncSettings::default(),
            &accounts,
            Duration::ZERO,
        );
        assert_eq!(separate.directories_read, 0);
    }

    #[test]
    fn recent_transcript_must_not_be_copied() {
        let (_, accounts) = fixture();
        assert_eq!(
            mirror_accounts(&sharing_all(), &accounts, Duration::from_secs(3600),).copied,
            0
        );
    }

    #[test]
    fn ignores_different_providers_and_invalid_roots() {
        let (_, mut accounts) = fixture();
        let foreign = accounts[0].history_root.clone();
        accounts[1].provider = "claude";
        assert_eq!(
            mirror_accounts(&sharing_all(), &accounts, Duration::ZERO).copied,
            0
        );
        accounts[1].provider = "codex";
        accounts[1].history_root = foreign;
        assert_eq!(
            mirror_accounts(&sharing_all(), &accounts, Duration::ZERO).copied,
            0
        );
    }
}
