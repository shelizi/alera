//! Atomic storage foundation for moving an existing terminal tab between
//! workspaces. Deliberately separate from the live PTY move: the host must
//! coordinate its Session, hooks, and shutdown owner before exposing an RPC.

use std::collections::BTreeSet;

use anyhow::{bail, ensure, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::Row;

use super::{format_timestamp, tab_from_row, RuntimeStore, WorkspaceTabRecord};

const TRANSFER_RECEIPT_KEY: &str = "lastTerminalTabTransfer";
const EXECUTION_WORKSPACE_KEY: &str = "terminalExecutionWorkspaceId";
const EXECUTION_LAUNCH_DIRECTORY_KEY: &str = "terminalExecutionWorkingDirectory";

impl RuntimeStore {
    /// Compare-and-swap both serialized layouts and tab ownership inside one
    /// SQLite transaction. The caller supplies *both* layout snapshots and
    /// replacements: stale clients fail rather than overwrite another pane
    /// edit or leave half a transfer behind. A stable mutation id handles
    /// timeout/replay after the database commit.
    ///
    /// This only updates persistence. Do not expose it directly to UI:
    /// moving the live host Session's display owner, pulse/bookkeeping, and
    /// agent execution scope is a separate required step.
    #[allow(clippy::too_many_arguments)]
    pub async fn transfer_terminal_tab_storage(
        &self,
        tab_id: &str,
        source_workspace_id: &str,
        target_workspace_id: &str,
        client_mutation_id: &str,
        expected_source_layout: Option<&Value>,
        expected_target_layout: Option<&Value>,
        source_layout: &Value,
        target_layout: &Value,
        execution_launch_directory: &str,
    ) -> Result<WorkspaceTabRecord> {
        ensure!(
            !tab_id.trim().is_empty()
                && !client_mutation_id.trim().is_empty()
                && !source_workspace_id.trim().is_empty()
                && !target_workspace_id.trim().is_empty(),
            "Terminal transfer identifiers must not be empty."
        );
        ensure!(
            source_workspace_id != target_workspace_id,
            "A terminal transfer requires distinct workspaces."
        );
        let source_ids = tab_ids_in_layout(source_layout, source_workspace_id)?;
        let target_ids = tab_ids_in_layout(target_layout, target_workspace_id)?;
        ensure!(
            !source_ids.contains(tab_id) && target_ids.contains(tab_id),
            "Transferred terminal tab must appear once in the destination layout only."
        );

        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, workspaceId, kind, title, createdAt, updatedAt, payloadJson \
             FROM workspaceTabs WHERE id = ?",
        )
        .bind(tab_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            bail!("Terminal tab does not exist.");
        };
        let mut tab = tab_from_row(row)?;
        ensure!(tab.kind == "terminal", "Only terminal tabs can transfer.");

        // The preceding request may have committed before its reply was lost.
        // Replay remains safe only for the *exact same* source, destination,
        // and mutation identity; never turn an unrelated move into success.
        let receipt = &tab.payload[TRANSFER_RECEIPT_KEY];
        if tab.workspace_id == target_workspace_id
            && receipt["id"].as_str() == Some(client_mutation_id)
            && receipt["from"].as_str() == Some(source_workspace_id)
            && receipt["to"].as_str() == Some(target_workspace_id)
        {
            return Ok(tab);
        }
        ensure!(
            tab.workspace_id == source_workspace_id,
            "Terminal has already moved or its source workspace changed."
        );

        let source_host = active_workspace_host(&mut tx, source_workspace_id).await?;
        let target_host = active_workspace_host(&mut tx, target_workspace_id).await?;
        ensure!(
            source_host == target_host,
            "Terminal transfer across Runtime hosts is not supported."
        );

        let actual_source = persisted_layout(&mut tx, source_workspace_id).await?;
        let actual_target = persisted_layout(&mut tx, target_workspace_id).await?;
        ensure!(
            actual_source.as_ref() == expected_source_layout
                && actual_target.as_ref() == expected_target_layout,
            "A workspace layout changed; refresh before transferring the terminal."
        );
        let previous_source_group_id =
            expected_source_layout.and_then(|layout| layout_group_for_tab(layout, tab_id));

        // Compare against the *actual* authoritative tab registry, not just
        // the client-supplied layout. No unrelated tab may be lost/duplicated.
        let mut expected_source_ids = workspace_tab_ids(&mut tx, source_workspace_id).await?;
        let mut expected_target_ids = workspace_tab_ids(&mut tx, target_workspace_id).await?;
        ensure!(
            expected_source_ids.remove(tab_id) && expected_target_ids.insert(tab_id.to_owned()),
            "Workspace tabs changed while the transfer was prepared."
        );
        ensure!(
            source_ids == expected_source_ids && target_ids == expected_target_ids,
            "Updated layouts must contain exactly the remaining source and destination tabs."
        );

        if !tab.payload.is_object() {
            tab.payload = json!({});
        }
        if tab.payload[EXECUTION_WORKSPACE_KEY].is_null() {
            // Process CWD, inherited environment, and agent hook identity
            // remain tied to where the terminal was launched, even after
            // the tab is displayed in another project.
            tab.payload[EXECUTION_WORKSPACE_KEY] = json!(source_workspace_id);
        }
        if tab.payload[EXECUTION_LAUNCH_DIRECTORY_KEY].is_null()
            && !execution_launch_directory.trim().is_empty()
        {
            // This is the original *launch* directory, not a claim that a
            // running shell has not subsequently changed its CWD.
            tab.payload[EXECUTION_LAUNCH_DIRECTORY_KEY] = json!(execution_launch_directory);
        }
        tab.payload[TRANSFER_RECEIPT_KEY] = json!({
            "id": client_mutation_id,
            "from": source_workspace_id,
            "to": target_workspace_id,
            "sourceGroupId": previous_source_group_id
        });
        tab.workspace_id = target_workspace_id.to_owned();
        tab.updated_at = Utc::now();
        let updated = sqlx::query(
            "UPDATE workspaceTabs SET workspaceId = ?, updatedAt = ?, payloadJson = ? \
             WHERE id = ? AND workspaceId = ?",
        )
        .bind(&tab.workspace_id)
        .bind(format_timestamp(tab.updated_at))
        .bind(serde_json::to_string(&tab.payload)?)
        .bind(tab_id)
        .bind(source_workspace_id)
        .execute(&mut *tx)
        .await?;
        ensure!(updated.rows_affected() == 1, "Terminal ownership changed.");

        for (workspace_id, layout) in [
            (source_workspace_id, source_layout),
            (target_workspace_id, target_layout),
        ] {
            sqlx::query(
                "INSERT INTO workbenchLayouts (workspaceId, dataJson) VALUES (?, ?) \
                 ON CONFLICT(workspaceId) DO UPDATE SET dataJson = excluded.dataJson",
            )
            .bind(workspace_id)
            .bind(serde_json::to_string(layout)?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(tab)
    }
}

fn layout_group_for_tab<'a>(layout: &'a Value, tab_id: &str) -> Option<&'a str> {
    layout
        .get("groups")?
        .as_object()?
        .iter()
        .find_map(|(id, group)| {
            group
                .get("tabIds")?
                .as_array()?
                .iter()
                .any(|tab| tab.as_str() == Some(tab_id))
                .then_some(id.as_str())
        })
}

fn tab_ids_in_layout(layout: &Value, workspace_id: &str) -> Result<BTreeSet<String>> {
    ensure!(
        layout["workspaceId"].as_str() == Some(workspace_id),
        "Layout belongs to a different workspace."
    );
    let groups = layout["groups"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Transfer layout is missing pane groups."))?;
    ensure!(
        !groups.is_empty() && layout["root"].is_object(),
        "Transfer layout has no valid split tree."
    );
    let mut root_group_ids = BTreeSet::new();
    collect_layout_group_ids(&layout["root"], &mut root_group_ids)?;
    ensure!(
        root_group_ids == groups.keys().cloned().collect(),
        "Transfer split tree and pane groups disagree."
    );
    let active = layout["activeGroupId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Transfer layout has no active pane group."))?;
    ensure!(
        groups.contains_key(active),
        "Active pane group does not exist."
    );
    let mut tab_ids = BTreeSet::new();
    for (group_id, group) in groups {
        ensure!(
            group["id"].as_str() == Some(group_id.as_str()),
            "Pane group identity mismatch."
        );
        let ids = group["tabIds"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Pane group has no tab list."))?;
        if !ids.is_empty() {
            let active_tab = group["activeTabId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Nonempty pane has no active tab."))?;
            ensure!(
                ids.iter().any(|id| id.as_str() == Some(active_tab)),
                "Pane active tab is not in its tab list."
            );
        }
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Pane group tab identifier is invalid."))?;
            ensure!(
                tab_ids.insert(id.to_owned()),
                "Duplicate tab in transfer layout."
            );
        }
    }
    Ok(tab_ids)
}

fn collect_layout_group_ids(node: &Value, ids: &mut BTreeSet<String>) -> Result<()> {
    match node["type"].as_str() {
        Some("leaf") => {
            let group_id = node["groupId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Split tree leaf has no pane group."))?;
            ensure!(
                ids.insert(group_id.to_owned()),
                "Split tree repeats a pane group."
            );
        }
        Some("split") => {
            collect_layout_group_ids(&node["first"], ids)?;
            collect_layout_group_ids(&node["second"], ids)?;
        }
        _ => bail!("Invalid split tree node in terminal transfer."),
    }
    Ok(())
}

async fn workspace_tab_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
) -> Result<BTreeSet<String>> {
    let rows = sqlx::query("SELECT id FROM workspaceTabs WHERE workspaceId = ?")
        .bind(workspace_id)
        .fetch_all(&mut **tx)
        .await?;
    rows.into_iter()
        .map(|row| -> Result<String> { Ok(row.try_get::<String, _>("id")?) })
        .collect()
}

async fn active_workspace_host(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
) -> Result<String> {
    let row = sqlx::query("SELECT hostId, status, archivedAt FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(row) = row else {
        bail!("Transfer workspace does not exist.");
    };
    ensure!(
        row.try_get::<String, _>("status")? == "active"
            && row.try_get::<Option<String>, _>("archivedAt")?.is_none(),
        "Archived or removed workspaces cannot receive or send a terminal."
    );
    Ok(row.try_get::<String, _>("hostId")?)
}

async fn persisted_layout(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
) -> Result<Option<Value>> {
    let row = sqlx::query("SELECT dataJson FROM workbenchLayouts WHERE workspaceId = ?")
        .bind(workspace_id)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(|row| -> Result<Value> {
        let raw: String = row.try_get("dataJson")?;
        Ok(serde_json::from_str(&raw)?)
    })
    .transpose()
}
