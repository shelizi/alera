//! Conservative account-to-account native transcript mirroring.
//! A provider may refuse to resume another account's mirrored transcript.
//! This only mirrors session files; it never copies credentials or settings.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use alera_core::runtime::{
    RuntimeAgentHistorySyncMode, RuntimeAgentHistorySyncSettings, RuntimeAgentQuotaSettings,
    RuntimeStore,
};
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

#[derive(Default, Debug, PartialEq, Eq)]
struct MirrorStats {
    copied: usize,
    skipped: usize,
    conflicts: usize,
    stale: usize,
}

pub(crate) fn spawn(runtime_store: RuntimeStore) -> tokio::task::JoinHandle<()> {
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
            match tokio::task::spawn_blocking(move || {
                mirror_accounts(&policy, &accounts, QUIET_PERIOD)
            })
            .await
            {
                Ok(stats) if stats.conflicts > 0 || stats.stale > 0 => tracing::warn!(
                    conflicts = stats.conflicts,
                    stale = stats.stale,
                    "agent history sync kept conflicting or stale transcripts unchanged"
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
fn mirror_accounts(
    policy: &RuntimeAgentHistorySyncSettings,
    accounts: &[HistoryAccount],
    quiet: Duration,
) -> MirrorStats {
    let mut stats = MirrorStats::default();
    let mut budget = MAX_FILES_PER_PASS;
    for source in accounts {
        for target in accounts {
            if budget == 0 {
                break;
            }
            if source.provider != target.provider
                || !policy.should_sync(source.provider, &source.account_id, &target.account_id)
            {
                continue;
            }
            mirror_pair(source, target, quiet, &mut budget, &mut stats);
        }
    }
    stats
}

fn mirror_pair(
    source: &HistoryAccount,
    target: &HistoryAccount,
    quiet: Duration,
    budget: &mut usize,
    stats: &mut MirrorStats,
) {
    if !real_directory(&source.history_root) || !real_directory(&target.history_root) {
        return;
    }
    let (Ok(source_root), Ok(target_root)) = (
        fs::canonicalize(&source.history_root),
        fs::canonicalize(&target.history_root),
    ) else {
        return;
    };
    // No recursive copying when accounts point at overlapping roots.
    if source_root.starts_with(&target_root) || target_root.starts_with(&source_root) {
        return;
    }
    let mut stack = vec![(source.history_root.clone(), PathBuf::new(), 0)];
    while let Some((directory, relative, depth)) = stack.pop() {
        if *budget == 0 {
            return;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            stats.skipped += 1;
            continue;
        };
        for entry in entries.flatten() {
            if *budget == 0 {
                return;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                stats.skipped += 1;
                continue;
            }
            let path = entry.path();
            let child = relative.join(entry.file_name());
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((path, child, depth + 1));
                }
                continue;
            }
            if !kind.is_file() || path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            // Previously mirrored files must not consume the copy budget.
            // Otherwise, large histories starve newer sessions indefinitely.
            let target_file = target.history_root.join(&child);
            if let Ok(existing) = fs::symlink_metadata(&target_file) {
                if let Ok(original) = fs::symlink_metadata(&path) {
                    if !existing.file_type().is_file() || !original.file_type().is_file() {
                        stats.conflicts += 1;
                    } else if existing.len() < original.len() {
                        if is_complete_line_prefix(&target_file, &path) {
                            stats.stale += 1;
                        } else {
                            stats.conflicts += 1;
                        }
                    } else if existing.len() > original.len()
                        && !is_complete_line_prefix(&path, &target_file)
                    {
                        stats.conflicts += 1;
                    }
                }
                continue;
            }
            *budget -= 1;
            match mirror_file(&path, &target.history_root, &child, quiet) {
                CopyOutcome::Copied => stats.copied += 1,
                CopyOutcome::Exists => stats.conflicts += 1,
                CopyOutcome::Skipped => stats.skipped += 1,
            }
        }
    }
}

/// Only classify an already existing transcript as a stale mirror if both
/// inputs are normal bounded files and the shorter JSONL ends at a line break.
/// Never mutate either file from this check.
fn is_complete_line_prefix(shorter: &Path, longer: &Path) -> bool {
    let (Ok(short), Ok(long)) = (fs::symlink_metadata(shorter), fs::symlink_metadata(longer))
    else {
        return false;
    };
    if !short.file_type().is_file()
        || !long.file_type().is_file()
        || short.len() == 0
        || short.len() >= long.len()
        || long.len() > MAX_FILE_BYTES
    {
        return false;
    }
    let (Ok(prefix), Ok(full)) = (fs::read(shorter), fs::read(longer)) else {
        return false;
    };
    prefix.last() == Some(&b'\n') && full.starts_with(&prefix)
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

    #[test]
    fn default_all_mirrors_only_transcripts_and_preserves_credentials() {
        let (dir, accounts) = fixture();
        let stats = mirror_accounts(
            &RuntimeAgentHistorySyncSettings::default(),
            &accounts,
            Duration::ZERO,
        );
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
        let (_, accounts) = fixture();
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
        let (_, accounts) = fixture();
        let target = accounts[1].history_root.join("2026/11/first.jsonl");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "other-account history").unwrap();
        let stats = mirror_accounts(
            &RuntimeAgentHistorySyncSettings::default(),
            &accounts,
            Duration::ZERO,
        );
        assert_eq!(stats.copied, 0);
        assert!(stats.conflicts >= 1);
        assert_eq!(fs::read_to_string(target).unwrap(), "other-account history");
    }

    #[test]
    fn later_messages_are_reported_as_stale_and_never_overwrite_a_native_copy() {
        let (_, accounts) = fixture();
        let policy = RuntimeAgentHistorySyncSettings::default();
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
    fn recent_transcript_must_not_be_copied() {
        let (_, accounts) = fixture();
        assert_eq!(
            mirror_accounts(
                &RuntimeAgentHistorySyncSettings::default(),
                &accounts,
                Duration::from_secs(3600),
            )
            .copied,
            0
        );
    }

    #[test]
    fn ignores_different_providers_and_invalid_roots() {
        let (_, mut accounts) = fixture();
        let foreign = accounts[0].history_root.clone();
        accounts[1].provider = "claude";
        assert_eq!(
            mirror_accounts(
                &RuntimeAgentHistorySyncSettings::default(),
                &accounts,
                Duration::ZERO
            )
            .copied,
            0
        );
        accounts[1].provider = "codex";
        accounts[1].history_root = foreign;
        assert_eq!(
            mirror_accounts(
                &RuntimeAgentHistorySyncSettings::default(),
                &accounts,
                Duration::ZERO
            )
            .copied,
            0
        );
    }
}
