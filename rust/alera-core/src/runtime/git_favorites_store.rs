//! One Rust owner for Classic/GPUI Git branch favorites.
//! Identity is a validated (Host project ID, optional POSIX relative Git
//! root), never a Native absolute worktree path or a guessed Git remote.
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Project, ProjectKind, RuntimeStore};

const KEY: &str = "git.favorites.scopes.v1";
const MAX_SCOPES: usize = 512;
const MAX_BRANCHES: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FavoriteCatalog {
    version: u32,
    scopes: BTreeMap<String, Vec<String>>,
}

impl Default for FavoriteCatalog {
    fn default() -> Self {
        Self {
            version: 1,
            scopes: BTreeMap::new(),
        }
    }
}

impl FavoriteCatalog {
    fn decode(raw: Option<String>) -> Result<Self> {
        let Some(raw) = raw else {
            return Ok(Self::default());
        };
        let result: Self = serde_json::from_str(&raw)
            .context("Stored Git favorites metadata is malformed; refusing overwrite")?;
        ensure!(
            result.version == 1 && result.scopes.len() <= MAX_SCOPES,
            "Unsupported Git favorites schema or number of scopes"
        );
        for (key, branches) in &result.scopes {
            parse_scope(key)?;
            validate_branches(branches)?;
        }
        Ok(result)
    }

    fn snapshot(&self, changed: bool) -> Value {
        json!({"scopes":self.scopes, "changed":changed})
    }
}

fn parse_scope(key: &str) -> Result<(String, Option<String>)> {
    ensure!(key.len() <= 1200, "Git favorites scope is too long");
    let (project_id, relative): (String, Option<String>) =
        serde_json::from_str(key).context("Invalid Git favorites project identity")?;
    ensure!(
        !project_id.is_empty()
            && project_id.len() <= 256
            && !project_id.chars().any(char::is_control),
        "Invalid project ID"
    );
    if let Some(root) = relative.as_deref() {
        ensure!(
            !root.is_empty()
                && root.len() <= 1024
                && !root.starts_with('/')
                && !root.contains('\\')
                && !root.contains(':')
                && !root.contains('\0')
                && root.split('/').all(|part| {
                    !part.is_empty()
                        && part != "."
                        && part != ".."
                        && !part.chars().any(char::is_control)
                }),
            "Invalid nested Git root"
        );
    }
    // Reject equivalent but non-canonical JSON identities.
    ensure!(
        serde_json::to_string(&(project_id.as_str(), &relative))? == key,
        "Noncanonical Git favorites scope"
    );
    Ok((project_id, relative))
}

fn validate_branches(branches: &[String]) -> Result<()> {
    ensure!(
        branches.len() <= MAX_BRANCHES,
        "Too many favorite Git branches"
    );
    let mut unique = HashSet::new();
    for branch in branches {
        ensure!(
            !branch.is_empty()
                && branch.len() <= 256
                && !branch.starts_with('-')
                && !branch.chars().any(char::is_control)
                && unique.insert(branch.as_str()),
            "Invalid or duplicate Git favorite branch"
        );
    }
    Ok(())
}

async fn catalog_in(tx: &mut sqlx::SqliteConnection) -> Result<FavoriteCatalog> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM runtimeMetadata WHERE key = ?")
        .bind(KEY)
        .fetch_optional(&mut *tx)
        .await?;
    FavoriteCatalog::decode(raw)
}

async fn persist_in(tx: &mut sqlx::SqliteConnection, catalog: &FavoriteCatalog) -> Result<()> {
    let raw = serde_json::to_string(catalog)?;
    sqlx::query("INSERT INTO runtimeMetadata (key,value,updatedAt) VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updatedAt=excluded.updatedAt")
        .bind(KEY).bind(raw).execute(&mut *tx).await?;
    Ok(())
}

#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}
#[cfg(not(windows))]
fn same_path(a: &Path, b: &Path) -> bool {
    a == b
}

impl RuntimeStore {
    /// Snapshot of reviewed, non-secret Git favorites, keyed exactly as
    /// Classic's branchFavoritesScopeKey(projectId, relativeGitRoot).
    pub async fn git_favorites_snapshot(&self) -> Result<Value> {
        let mut tx = self.pool().begin().await?;
        let value = catalog_in(&mut tx).await?.snapshot(false);
        tx.commit().await?;
        Ok(value)
    }

    /// Classic migration is insert-only. Missing/retired project IDs are
    /// skipped, not recreated. Entries explicitly cleared in Rust remain
    /// present as [] tombstones and will never be reseeded from old Drift.
    pub async fn git_favorites_seed_classic(&self, legacy: &Value) -> Result<Value> {
        let provided = legacy
            .as_object()
            .context("Classic favorites must be a scope map")?;
        ensure!(
            provided.len() <= MAX_SCOPES,
            "Too many legacy Git favorites scopes"
        );
        let existing_projects: std::collections::HashMap<String, ProjectKind> = self
            .list_projects()
            .await?
            .into_iter()
            .map(|p| (p.id, p.kind))
            .collect();
        let mut accepted = Vec::new();
        let mut skipped = Vec::new();
        for (key, values) in provided {
            let (project_id, relative_root) = match parse_scope(key) {
                Ok(id) => id,
                Err(_) => {
                    skipped.push(key.clone());
                    continue;
                }
            };
            let branches: Vec<String> = match serde_json::from_value(values.clone()) {
                Ok(v) => v,
                Err(_) => {
                    skipped.push(key.clone());
                    continue;
                }
            };
            if !existing_projects
                .get(&project_id)
                .is_some_and(|kind| *kind == ProjectKind::GitRepository || relative_root.is_some())
                || validate_branches(&branches).is_err()
            {
                skipped.push(key.clone());
                continue;
            }
            accepted.push((key.clone(), branches));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut catalog = catalog_in(&mut tx).await?;
        let mut changed = false;
        for (key, branches) in accepted {
            if catalog.scopes.len() >= MAX_SCOPES && !catalog.scopes.contains_key(&key) {
                bail!("Shared Git favorites catalog is full");
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = catalog.scopes.entry(key) {
                entry.insert(branches);
                changed = true;
            }
        }
        if changed {
            persist_in(&mut tx, &catalog).await?;
        }
        tx.commit().await?;
        let mut result = catalog.snapshot(changed);
        result["skipped"] = json!(skipped);
        Ok(result)
    }

    /// CAS at the exact repository scope. An empty list is persisted to
    /// prevent a subsequent old frontend import from reviving cleared pins.
    pub async fn git_favorites_compare_and_set(
        &self,
        scope_key: &str,
        expected: &[String],
        next: &[String],
    ) -> Result<Value> {
        let (project_id, relative_root) = parse_scope(scope_key)?;
        validate_branches(expected)?;
        validate_branches(next)?;
        let project = self.find_project(&project_id).await?;
        ensure!(
            project
                .is_some_and(|p| p.kind == ProjectKind::GitRepository || relative_root.is_some()),
            "Git favorites project is missing or lacks a nested Git root"
        );
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        // Project might be removed concurrently; constrain the mutation in
        // the same transaction, not only through the initial lookup.
        let exists: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM projects WHERE id = ? AND (kind = 'gitRepository' OR ? = 1) LIMIT 1",
        )
        .bind(&project_id)
        .bind(i32::from(relative_root.is_some()))
        .fetch_optional(&mut *tx)
        .await?;
        ensure!(exists.is_some(), "Git favorites project was removed");
        let mut catalog = catalog_in(&mut tx).await?;
        let current = catalog
            .scopes
            .get(scope_key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if current != expected {
            bail!("git_favorites_revision_conflict");
        }
        let changed = current != next || !catalog.scopes.contains_key(scope_key);
        if changed {
            ensure!(
                catalog.scopes.contains_key(scope_key) || catalog.scopes.len() < MAX_SCOPES,
                "Too many shared Git favorites scopes"
            );
            catalog.scopes.insert(scope_key.to_owned(), next.to_vec());
            persist_in(&mut tx, &catalog).await?;
        }
        tx.commit().await?;
        Ok(json!({"scopeKey":scope_key,"branches":next,"changed":changed}))
    }

    /// Only exact, canonical, Host-registered Project/Workspace roots are
    /// accepted from Native. Prevents a Native path from aliasing the wrong
    /// Project, another worktree, or nested Git repository.
    pub async fn git_favorites_scope_for_workspace(&self, root: &str) -> Result<String> {
        ensure!(
            !root.is_empty() && root.len() <= 4096,
            "Invalid Native workspace path"
        );
        let canonical =
            dunce::canonicalize(root).context("Native workspace directory unavailable")?;
        ensure!(canonical.is_dir(), "Native workspace must be a directory");
        let projects = self.list_projects().await?;
        ensure!(
            projects.len() <= 512,
            "Too many Host projects to resolve safely"
        );
        let workspaces = self.list_all_workspaces().await?;
        ensure!(
            workspaces.len() <= 4096,
            "Too many Host workspaces to resolve safely"
        );
        let ids: HashSet<String> = projects
            .iter()
            .filter(|p| p.kind == ProjectKind::GitRepository)
            .filter_map(|p| {
                dunce::canonicalize(&p.repo_path)
                    .ok()
                    .filter(|registered| same_path(registered, &canonical))
                    .map(|_| p.id.clone())
            })
            .chain(workspaces.iter().filter_map(|w| {
                dunce::canonicalize(&w.path)
                    .ok()
                    .filter(|registered| same_path(registered, &canonical))
                    .map(|_| w.project_id.clone())
            }))
            .collect();
        ensure!(
            ids.len() == 1,
            "Workspace path is unregistered or ambiguous in Runtime Host"
        );
        let id = ids.into_iter().next().unwrap();
        let project: &Project = projects
            .iter()
            .find(|p| p.id == id && p.kind == ProjectKind::GitRepository)
            .context("Workspace refers to a missing or non-Git Host project")?;
        let _ = project;
        Ok(serde_json::to_string(&(id, Option::<String>::None))?)
    }

    pub async fn git_favorites_for_workspace(&self, root: &str) -> Result<Value> {
        let key = self.git_favorites_scope_for_workspace(root).await?;
        let mut tx = self.pool().begin().await?;
        let catalog = catalog_in(&mut tx).await?;
        let present = catalog.scopes.contains_key(&key);
        let branches = catalog.scopes.get(&key).cloned().unwrap_or_default();
        tx.commit().await?;
        Ok(json!({"scopeKey":key,"branches":branches,"present":present}))
    }

    pub async fn git_favorites_cas_for_workspace(
        &self,
        root: &str,
        expected: &[String],
        next: &[String],
    ) -> Result<Value> {
        let key = self.git_favorites_scope_for_workspace(root).await?;
        self.git_favorites_compare_and_set(&key, expected, next)
            .await
    }

    /// Native's old per-root JSON is imported once per Host repository scope.
    pub async fn git_favorites_seed_native(&self, root: &str, legacy: &[String]) -> Result<Value> {
        let key = self.git_favorites_scope_for_workspace(root).await?;
        validate_branches(legacy)?;
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut catalog = catalog_in(&mut tx).await?;
        let changed = !catalog.scopes.contains_key(&key);
        if changed {
            ensure!(
                catalog.scopes.len() < MAX_SCOPES,
                "Too many shared Git favorites scopes"
            );
            catalog.scopes.insert(key.clone(), legacy.to_vec());
            persist_in(&mut tx, &catalog).await?;
        }
        let branches = catalog.scopes.get(&key).cloned().unwrap_or_default();
        tx.commit().await?;
        Ok(json!({"scopeKey":key,"branches":branches,"present":true,"changed":changed}))
    }
}
