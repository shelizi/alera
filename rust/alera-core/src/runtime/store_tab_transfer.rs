//! Atomic storage foundation for moving an existing terminal tab between
//! workspaces. Deliberately separate from the live PTY move: the host must
//! coordinate its Session, hooks, and shutdown owner before exposing an RPC.

use std::collections::BTreeSet;

use anyhow::{bail, ensure, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::{format_timestamp, tab_from_row, RuntimeStore, WorkspaceTabRecord};

const TRANSFER_RECEIPT_KEY: &str = "lastTerminalTabTransfer";
const TRANSFER_EXIT_EVIDENCE_KEY: &str = "lastTerminalTabExit";
const EXECUTION_WORKSPACE_KEY: &str = "terminalExecutionWorkspaceId";
const EXECUTION_LAUNCH_DIRECTORY_KEY: &str = "terminalExecutionWorkingDirectory";

impl RuntimeStore {
    /// The only storage path for a moved Tab explicitly closed by its current
    /// Host's verified original live PTY and single attached UI view.
    ///
    /// The Host actor MUST verify the actual live Session generation, latest
    /// transfer revision, and only attached View before dispatching this
    /// request. This Store layer repeats all durable identity checks, both
    /// complete pane registries and the original View token's presence; it
    /// cannot independently prove OS process liveness.
    ///
    /// Atomically delete the Tab, remove it from the destination pane, and
    /// append a permanent audit. A missing PTY / observed Exit is NOT an
    /// acceptable substitute for live-view authorization.
    #[allow(clippy::too_many_arguments)]
    pub async fn remove_transferred_terminal_after_verified_live_view(
        &self,
        tab_id: &str,
        session_id: &str,
        expected_workspace_id: &str,
        expected_transfer_mutation_id: &str,
        expected_original_pty_token: &str,
        expected_view_lease_id: &str,
    ) -> Result<()> {
        ensure!(
            [
                tab_id,
                session_id,
                expected_workspace_id,
                expected_transfer_mutation_id,
                expected_original_pty_token,
                expected_view_lease_id,
            ]
            .iter()
            .all(|id| !id.trim().is_empty()),
            "Explicit transferred Terminal retirement requires original Session, PTY, View and receipt."
        );
        let (host_epoch, number) = expected_original_pty_token
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Original PTY instance has no Host generation."))?;
        uuid::Uuid::parse_str(host_epoch)?;
        ensure!(
            number.parse::<u64>().is_ok(),
            "Original PTY instance identifier is malformed."
        );
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, workspaceId, kind, title, createdAt, updatedAt, payloadJson \
             FROM workspaceTabs WHERE id = ?",
        )
        .bind(tab_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transferred Terminal no longer exists."))?;
        let original_payload: String = row.try_get("payloadJson")?;
        let tab = tab_from_row(row)?;
        let receipt = &tab.payload[TRANSFER_RECEIPT_KEY];
        let persisted_session_id = tab.payload["terminalSessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(tab_id);
        let source = receipt["from"].as_str().unwrap_or_default();
        let target = receipt["to"].as_str().unwrap_or_default();
        ensure!(
            tab.kind == "terminal"
                && tab.workspace_id == expected_workspace_id
                && target == expected_workspace_id
                && !source.is_empty()
                && source != target
                && persisted_session_id == session_id
                && receipt["id"].as_str() == Some(expected_transfer_mutation_id)
                && receipt["ptyInstanceToken"].as_str() == Some(expected_original_pty_token)
                && tab.payload.get(TRANSFER_EXIT_EVIDENCE_KEY).is_none(),
            "Moved Tab original PTY/owner/receipt differs or exit evidence requires manual recovery."
        );
        let original_source = persisted_layout(&mut tx, source)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Transferred source layout is missing."))?;
        let target_layout_raw: String =
            sqlx::query_scalar("SELECT dataJson FROM workbenchLayouts WHERE workspaceId = ?")
                .bind(target)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Transferred target layout is missing."))?;
        let original_target: Value = serde_json::from_str(&target_layout_raw)?;
        let source_ids = tab_ids_in_layout(&original_source, source)?;
        let target_ids = tab_ids_in_layout(&original_target, target)?;
        ensure!(
            !source_ids.contains(tab_id)
                && target_ids.contains(tab_id)
                && source_ids == workspace_tab_ids(&mut tx, source).await?
                && target_ids == workspace_tab_ids(&mut tx, target).await?,
            "Both complete transferred Terminal layouts and authoritative Tab registries must match."
        );
        let next_target = layout_after_retiring_tab(&original_target, target, tab_id)?;
        let mut remaining_target_ids = target_ids;
        ensure!(
            remaining_target_ids.remove(tab_id),
            "Retired Tab was not in its target layout."
        );
        ensure!(
            tab_ids_in_layout(&next_target, target)? == remaining_target_ids,
            "Retiring Terminal would orphan or duplicate other pane tabs."
        );

        let removed = sqlx::query(
            "DELETE FROM workspaceTabs WHERE id = ? AND workspaceId = ? AND payloadJson = ?",
        )
        .bind(tab_id)
        .bind(target)
        .bind(original_payload)
        .execute(&mut *tx)
        .await?;
        ensure!(
            removed.rows_affected() == 1,
            "Transferred Terminal changed during retirement."
        );
        let updated = sqlx::query(
            "UPDATE workbenchLayouts SET dataJson = ? \
             WHERE workspaceId = ? AND dataJson = ?",
        )
        .bind(serde_json::to_string(&next_target)?)
        .bind(target)
        .bind(target_layout_raw)
        .execute(&mut *tx)
        .await?;
        ensure!(
            updated.rows_affected() == 1,
            "Destination pane changed during original-PTY retirement."
        );
        sqlx::query("DELETE FROM agentProfileLaunchReceipts WHERE tabId = ?")
            .bind(tab_id)
            .execute(&mut *tx)
            .await?;
        // Never store the secret bearer View token itself in the audit.
        let view_digest = format!("{:x}", Sha256::digest(expected_view_lease_id.as_bytes()));
        sqlx::query(
            "INSERT INTO terminalTabRetirementAudits \
             (id, tabId, sessionId, sourceWorkspaceId, targetWorkspaceId, \
              transferMutationId, originalPtyInstanceToken, viewLeaseSha256, \
              reason, recordedAtUtc) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(tab_id)
        .bind(session_id)
        .bind(source)
        .bind(target)
        .bind(expected_transfer_mutation_id)
        .bind(expected_original_pty_token)
        .bind(view_digest)
        .bind("explicit_verified_original_live_pty_only_view")
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// A lost-reply transfer is not acknowledged merely because the Tab row
    /// and its PTY instance match. Read *both* persisted layouts and the
    /// authoritative Tab registries in one SQLite snapshot: neither pane
    /// may retain a stale duplicate or silently omit another Tab.
    ///
    /// This read-only check does NOT prove a Host process survived a crash,
    /// nor authorize retirement or another transfer.
    #[allow(clippy::too_many_arguments)]
    pub async fn verify_terminal_transfer_layout_and_receipt(
        &self,
        tab_id: &str,
        session_id: &str,
        source_workspace_id: &str,
        target_workspace_id: &str,
        mutation_id: &str,
        pty_instance_token: &str,
    ) -> Result<Option<WorkspaceTabRecord>> {
        self.transferred_terminal_snapshot(
            tab_id,
            session_id,
            source_workspace_id,
            target_workspace_id,
            mutation_id,
            pty_instance_token,
            false,
        )
        .await
    }

    /// Read-only, SAME-TRANSACTION preflight for an eventual explicit
    /// retirement workflow. Requiring a recorded normal Exit here never
    /// authorizes termination, deletion, clearing Journals, or recovering a
    /// vanished PTY: a live Host/session/view authorization and a fresh
    /// write-time compare-and-swap still have to be designed.
    #[allow(clippy::too_many_arguments)]
    pub async fn inspect_transferred_terminal_retirement_evidence(
        &self,
        tab_id: &str,
        session_id: &str,
        source_workspace_id: &str,
        target_workspace_id: &str,
        mutation_id: &str,
        pty_instance_token: &str,
    ) -> Result<Option<WorkspaceTabRecord>> {
        self.transferred_terminal_snapshot(
            tab_id,
            session_id,
            source_workspace_id,
            target_workspace_id,
            mutation_id,
            pty_instance_token,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn transferred_terminal_snapshot(
        &self,
        tab_id: &str,
        session_id: &str,
        source_workspace_id: &str,
        target_workspace_id: &str,
        mutation_id: &str,
        pty_instance_token: &str,
        require_observed_exit: bool,
    ) -> Result<Option<WorkspaceTabRecord>> {
        if tab_id.is_empty()
            || session_id.is_empty()
            || mutation_id.is_empty()
            || pty_instance_token.is_empty()
            || source_workspace_id == target_workspace_id
        {
            return Ok(None);
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, workspaceId, kind, title, createdAt, updatedAt, payloadJson \
             FROM workspaceTabs WHERE id = ?",
        )
        .bind(tab_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let tab = tab_from_row(row)?;
        let receipt = &tab.payload[TRANSFER_RECEIPT_KEY];
        let stored_session = tab.payload["terminalSessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(tab_id);
        if tab.kind != "terminal"
            || tab.workspace_id != target_workspace_id
            || stored_session != session_id
            || receipt["id"].as_str() != Some(mutation_id)
            || receipt["from"].as_str() != Some(source_workspace_id)
            || receipt["to"].as_str() != Some(target_workspace_id)
            || receipt["ptyInstanceToken"].as_str() != Some(pty_instance_token)
        {
            return Ok(None);
        }
        if require_observed_exit {
            let evidence = &tab.payload[TRANSFER_EXIT_EVIDENCE_KEY];
            let timestamp_valid = evidence["observedAtUtc"]
                .as_str()
                .is_some_and(|date| chrono::DateTime::parse_from_rfc3339(date).is_ok());
            if evidence["transferMutationId"].as_str() != Some(mutation_id)
                || evidence["workspaceId"].as_str() != Some(target_workspace_id)
                || evidence["sessionId"].as_str() != Some(session_id)
                || evidence["ptyInstanceToken"].as_str() != Some(pty_instance_token)
                || evidence["exitCode"].as_i64().is_none()
                || !timestamp_valid
            {
                return Ok(None);
            }
        }
        let (Some(source_layout), Some(target_layout)) = (
            persisted_layout(&mut tx, source_workspace_id).await?,
            persisted_layout(&mut tx, target_workspace_id).await?,
        ) else {
            return Ok(None);
        };
        let (Ok(source_ids), Ok(target_ids)) = (
            tab_ids_in_layout(&source_layout, source_workspace_id),
            tab_ids_in_layout(&target_layout, target_workspace_id),
        ) else {
            return Ok(None);
        };
        if source_ids.contains(tab_id) || !target_ids.contains(tab_id) {
            return Ok(None);
        }
        let stored_source_ids = workspace_tab_ids(&mut tx, source_workspace_id).await?;
        let stored_target_ids = workspace_tab_ids(&mut tx, target_workspace_id).await?;
        Ok((source_ids == stored_source_ids && target_ids == stored_target_ids).then_some(tab))
    }

    /// Persist one Host-observed exit of the *same original PTY generation*
    /// after a transferred Terminal ends normally. Never infer an exit from a
    /// missing Session or a missing Host process after a crash. This is
    /// diagnostic evidence, NOT permission to delete the Tab or to treat a
    /// lost transfer response as a confirmed successful live-PTY move.
    pub async fn record_transferred_terminal_exit(
        &self,
        tab_id: &str,
        session_id: &str,
        workspace_id: &str,
        pty_instance_token: &str,
        exit_code: i32,
    ) -> Result<bool> {
        ensure!(
            !tab_id.trim().is_empty()
                && !session_id.trim().is_empty()
                && !workspace_id.trim().is_empty()
                && !pty_instance_token.trim().is_empty(),
            "Transferred Terminal exit evidence requires exact original identity."
        );
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            "SELECT id, workspaceId, kind, title, createdAt, updatedAt, payloadJson \
             FROM workspaceTabs WHERE id = ?",
        )
        .bind(tab_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(false);
        };
        let original_payload: String = row.try_get("payloadJson")?;
        let mut tab = tab_from_row(row)?;
        let receipt = &tab.payload[TRANSFER_RECEIPT_KEY];
        let original_session_id = tab.payload["terminalSessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(tab_id);
        if tab.kind != "terminal"
            || tab.workspace_id != workspace_id
            || original_session_id != session_id
            || receipt["to"].as_str() != Some(workspace_id)
            || receipt["ptyInstanceToken"].as_str() != Some(pty_instance_token)
        {
            return Ok(false);
        }
        let Some(transfer_id) = receipt["id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            return Ok(false);
        };
        // D15 treats the *presence* of a Host-owned exit key (even null) as
        // evidence of an interrupted generation. JSON indexing treats an
        // absent key and an explicit null identically; check key existence
        // instead, or this writer could erase a corrupted retained marker.
        if tab
            .payload
            .as_object()
            .is_some_and(|fields| fields.contains_key(TRANSFER_EXIT_EVIDENCE_KEY))
        {
            if !tab.payload[TRANSFER_EXIT_EVIDENCE_KEY].is_object() {
                return Ok(false);
            }
            let prior = &tab.payload[TRANSFER_EXIT_EVIDENCE_KEY];
            return Ok(
                prior["transferMutationId"].as_str() == Some(transfer_id.as_str())
                    && prior["ptyInstanceToken"].as_str() == Some(pty_instance_token)
                    && prior["sessionId"].as_str() == Some(session_id)
                    && prior["workspaceId"].as_str() == Some(workspace_id)
                    && prior["exitCode"].as_i64() == Some(i64::from(exit_code)),
            );
        }
        // The Actor observed this specific original PTY exit while the
        // persisted Tab and latest transfer receipt still matched.
        tab.payload[TRANSFER_EXIT_EVIDENCE_KEY] = json!({
            "transferMutationId": transfer_id,
            "sessionId": session_id,
            "workspaceId": workspace_id,
            "ptyInstanceToken": pty_instance_token,
            "exitCode": exit_code,
            "observedAtUtc": Utc::now().to_rfc3339(),
        });
        tab.updated_at = Utc::now();
        let update = sqlx::query(
            "UPDATE workspaceTabs SET payloadJson = ?, updatedAt = ? \
             WHERE id = ? AND workspaceId = ? AND payloadJson = ?",
        )
        .bind(serde_json::to_string(&tab.payload)?)
        .bind(format_timestamp(tab.updated_at))
        .bind(tab_id)
        .bind(workspace_id)
        .bind(original_payload)
        .execute(&mut *tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "Transferred Terminal Tab changed while its original PTY exit was observed."
        );
        tx.commit().await?;
        Ok(true)
    }

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
        pty_instance_token: &str,
    ) -> Result<WorkspaceTabRecord> {
        ensure!(
            !tab_id.trim().is_empty()
                && !client_mutation_id.trim().is_empty()
                && !source_workspace_id.trim().is_empty()
                && !target_workspace_id.trim().is_empty()
                && !pty_instance_token.trim().is_empty(),
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

        // Never overwrite interrupted original process evidence via direct
        // Store callers, even when the Host RPC has its own preflight.
        // A legitimate exited process requires a separate authenticated
        // retirement workflow; it cannot perform another live PTY move.
        ensure!(
            !tab.payload
                .as_object()
                .is_some_and(|fields| fields.contains_key(TRANSFER_EXIT_EVIDENCE_KEY)),
            "Terminal original PTY exit evidence needs audited recovery before another transfer."
        );
        let prior_receipt_present = tab
            .payload
            .as_object()
            .is_some_and(|fields| fields.contains_key(TRANSFER_RECEIPT_KEY));
        let receipt = &tab.payload[TRANSFER_RECEIPT_KEY];
        if prior_receipt_present {
            ensure!(
                ["id", "from", "to", "ptyInstanceToken"]
                    .iter()
                    .all(|field| receipt[*field]
                        .as_str()
                        .is_some_and(|value| !value.trim().is_empty()))
                    && receipt["ptyInstanceToken"].as_str() == Some(pty_instance_token)
                    && receipt["to"].as_str() == Some(tab.workspace_id.as_str()),
                "Stored Terminal receipt is malformed or belongs to another PTY generation."
            );
        }

        // The preceding request may have committed before its reply was lost.
        // Replay remains safe only for the *exact same* source, destination,
        // mutation and original PTY instance, never another reminted process.
        if tab.workspace_id == target_workspace_id
            && receipt["id"].as_str() == Some(client_mutation_id)
            && receipt["from"].as_str() == Some(source_workspace_id)
            && receipt["to"].as_str() == Some(target_workspace_id)
            && receipt["ptyInstanceToken"].as_str() == Some(pty_instance_token)
        {
            // An idempotent replay after a lost reply must not acknowledge
            // a partial/crashed layout commit or accept a different display
            // snapshot. Re-check BOTH sides and the full Tab registry in
            // the same SQLite read transaction before returning the receipt.
            let current_source = persisted_layout(&mut tx, source_workspace_id).await?;
            let current_target = persisted_layout(&mut tx, target_workspace_id).await?;
            ensure!(
                current_source.as_ref() == Some(source_layout)
                    && current_target.as_ref() == Some(target_layout),
                "Original Terminal transfer receipt exists, but one persisted layout changed."
            );
            ensure!(
                source_ids == workspace_tab_ids(&mut tx, source_workspace_id).await?
                    && target_ids == workspace_tab_ids(&mut tx, target_workspace_id).await?,
                "Original Terminal transfer receipt exists, but a persisted pane or Tab registry is inconsistent."
            );
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
        ensure_transfer_preserves_other_panes(
            expected_source_layout,
            expected_target_layout,
            source_layout,
            target_layout,
            tab_id,
        )?;

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
            "sourceGroupId": previous_source_group_id,
            // Session handles can be reminted to newly spawned PTYs. Tie
            // the receipt to the exact live instance at SQLite commit.
            "ptyInstanceToken": pty_instance_token
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

/// Equivalent to removing one Tab from WorkbenchLayout and pruning an empty
/// pane. The only permitted differences are this Tab, its active selection
/// and the strictly required split-tree pruning.
fn layout_after_retiring_tab(layout: &Value, workspace_id: &str, tab_id: &str) -> Result<Value> {
    let group_id = layout_group_for_tab(layout, tab_id)
        .ok_or_else(|| anyhow::anyhow!("Transferred Terminal has no destination pane."))?
        .to_owned();
    let mut next = layout.clone();
    let groups = next["groups"]
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Destination layout has no pane groups."))?;
    let old_group = groups
        .get(&group_id)
        .ok_or_else(|| anyhow::anyhow!("Destination pane was removed."))?;
    let kept = old_group["tabIds"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Destination pane has no Tab list."))?
        .iter()
        .filter(|value| value.as_str() != Some(tab_id))
        .cloned()
        .collect::<Vec<_>>();
    if kept.is_empty() {
        groups.remove(&group_id);
    } else {
        let old_active = old_group["activeTabId"].as_str();
        let active = old_active
            .filter(|id| kept.iter().any(|entry| entry.as_str() == Some(*id)))
            .map(|id| json!(id))
            .unwrap_or_else(|| kept[0].clone());
        let group = groups
            .get_mut(&group_id)
            .ok_or_else(|| anyhow::anyhow!("Destination pane changed during retirement."))?;
        group["tabIds"] = Value::Array(kept);
        group["activeTabId"] = active;
    }
    let remaining_groups = next["groups"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Destination pane groups vanished."))?;
    if remaining_groups.is_empty() {
        let default_id = format!("{workspace_id}/main");
        next["groups"] = json!({});
        next["groups"][&default_id] = json!({"id": default_id, "tabIds": [], "activeTabId": null});
        next["root"] = json!({"type": "leaf", "groupId": default_id});
        next["activeGroupId"] = json!(default_id);
    } else if !remaining_groups.contains_key(&group_id) {
        let retained = remaining_groups.keys().cloned().collect::<BTreeSet<_>>();
        let root = prune_transfer_source_tree(&layout["root"], &retained)?
            .ok_or_else(|| anyhow::anyhow!("Retired Terminal would remove every target split."))?;
        let first = first_transfer_tree_leaf(&root)
            .ok_or_else(|| anyhow::anyhow!("Pruned target split has no pane."))?
            .to_owned();
        let active = layout["activeGroupId"]
            .as_str()
            .filter(|id| retained.contains(*id))
            .unwrap_or(&first)
            .to_owned();
        next["root"] = root;
        next["activeGroupId"] = json!(active);
    }
    Ok(next)
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

/// The two-layout CAS verifies only tab membership. Restrict the submitted
/// replacement to removing the Terminal from one source pane and appending it
/// to one destination pane; a stale/modified client must not silently reorder
/// unrelated tabs or replace the destination split tree.
fn ensure_transfer_preserves_other_panes(
    old_source: Option<&Value>,
    old_target: Option<&Value>,
    new_source: &Value,
    new_target: &Value,
    tab_id: &str,
) -> Result<()> {
    if let Some(before) = old_source {
        let prior = before["groups"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Source layout has no pane groups."))?;
        let next = new_source["groups"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Transferred source has no pane groups."))?;
        ensure!(
            layout_group_for_tab(before, tab_id).is_some(),
            "Source layout no longer contains the transferred Terminal."
        );
        for (id, group) in prior {
            let remaining = group["tabIds"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Source pane has no tab list."))?
                .iter()
                .filter(|entry| entry.as_str() != Some(tab_id))
                .cloned()
                .collect::<Vec<_>>();
            if let Some(actual) = next.get(id) {
                let mut expected = group.clone();
                let old_active = group["activeTabId"].as_str();
                let retained_active = old_active
                    .filter(|active| remaining.iter().any(|item| item.as_str() == Some(*active)));
                expected["activeTabId"] = retained_active
                    .map(|active| json!(active))
                    .or_else(|| remaining.first().cloned())
                    .unwrap_or(Value::Null);
                expected["tabIds"] = Value::Array(remaining);
                ensure!(
                    *actual == expected,
                    "Terminal move unexpectedly changed a source pane."
                );
            } else {
                ensure!(
                    remaining.is_empty(),
                    "Terminal move cannot drop a source pane containing other tabs."
                );
            }
        }
        // removeTab() prunes empty groups; when the last tab leaves, it
        // constructs one empty default group. No new nonempty pane is legal.
        let source_empty = next
            .values()
            .all(|group| group["tabIds"].as_array().is_some_and(|ids| ids.is_empty()));
        let default_group = new_source["workspaceId"]
            .as_str()
            .map(|workspace_id| format!("{workspace_id}/main"));
        ensure!(
            next.keys().all(|id| {
                prior.contains_key(id)
                    || (source_empty
                        && next.len() == 1
                        && default_group.as_deref() == Some(id.as_str()))
            }),
            "Terminal move introduced an unrelated source pane."
        );
        // Mirror removeTab() exactly: prune empty source groups, promote the
        // surviving sibling and preserve every untouched split's ratio.
        let workspace_id = new_source["workspaceId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Source layout has no workspace identity."))?;
        if source_empty {
            let default_id = format!("{workspace_id}/main");
            ensure!(
                next.len() == 1
                    && next.contains_key(&default_id)
                    && new_source["root"] == json!({"type": "leaf", "groupId": default_id})
                    && new_source["activeGroupId"].as_str() == Some(default_id.as_str()),
                "Last source pane must become the empty default Workbench pane."
            );
        } else {
            ensure!(
                next.values().all(|group| group["tabIds"]
                    .as_array()
                    .is_some_and(|ids| !ids.is_empty())),
                "Terminal move left an unrelated empty source pane."
            );
            let retained = next.keys().cloned().collect::<BTreeSet<_>>();
            let pruned = prune_transfer_source_tree(&before["root"], &retained)?
                .ok_or_else(|| anyhow::anyhow!("Transfer removed every source split pane."))?;
            let first = first_transfer_tree_leaf(&pruned)
                .ok_or_else(|| anyhow::anyhow!("Pruned source layout has no pane."))?;
            let expected_active = before["activeGroupId"]
                .as_str()
                .filter(|id| retained.contains(*id))
                .unwrap_or(first);
            ensure!(
                new_source["root"] == pruned
                    && new_source["activeGroupId"].as_str() == Some(expected_active),
                "Terminal transfer changed unrelated split geometry or active pane."
            );
        }
    }
    if let Some(before) = old_target {
        let prior = before["groups"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Target layout has no pane groups."))?;
        let next = new_target["groups"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Transferred target has no pane groups."))?;
        let target_group = layout_group_for_tab(new_target, tab_id)
            .ok_or_else(|| anyhow::anyhow!("Transferred Terminal has no target pane."))?;
        ensure!(
            prior.len() == next.len()
                && prior.keys().all(|id| next.contains_key(id))
                && before["root"] == new_target["root"]
                && new_target["activeGroupId"].as_str() == Some(target_group),
            "Terminal transfer cannot replace destination panes."
        );
        for (id, group) in prior {
            let mut expected = group.clone();
            if id == target_group {
                let mut ids = group["tabIds"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Destination pane has no tab list."))?
                    .clone();
                ensure!(
                    !ids.iter().any(|entry| entry.as_str() == Some(tab_id)),
                    "Destination already contains the transferred Terminal."
                );
                ids.push(json!(tab_id));
                expected["tabIds"] = Value::Array(ids);
                expected["activeTabId"] = json!(tab_id);
            }
            ensure!(
                next.get(id) == Some(&expected),
                "Terminal transfer cannot reorder unrelated destination tabs."
            );
        }
    }
    Ok(())
}

/// Mirror WorkbenchLayoutNode.pruneGroups. When one child disappears, keep
/// its sibling verbatim; both children surviving must retain the original
/// parent split axis, ratio and any other metadata.
fn prune_transfer_source_tree(node: &Value, retained: &BTreeSet<String>) -> Result<Option<Value>> {
    match node["type"].as_str() {
        Some("leaf") => {
            let group = node["groupId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Source leaf has no pane group."))?;
            Ok(retained.contains(group).then(|| node.clone()))
        }
        Some("split") => {
            let first = prune_transfer_source_tree(&node["first"], retained)?;
            let second = prune_transfer_source_tree(&node["second"], retained)?;
            Ok(match (first, second) {
                (None, None) => None,
                (Some(one), None) | (None, Some(one)) => Some(one),
                (Some(first), Some(second)) => {
                    let mut combined = node.clone();
                    combined["first"] = first;
                    combined["second"] = second;
                    Some(combined)
                }
            })
        }
        _ => bail!("Invalid source split tree during Terminal transfer."),
    }
}

fn first_transfer_tree_leaf(node: &Value) -> Option<&str> {
    match node["type"].as_str()? {
        "leaf" => node["groupId"].as_str(),
        "split" => first_transfer_tree_leaf(&node["first"])
            .or_else(|| first_transfer_tree_leaf(&node["second"])),
        _ => None,
    }
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

/// Reject a stale legacy layout write if it would resurrect a moved
/// Terminal in the former workspace, or remove it from its current pane.
/// The caller holds the same SQLite transaction for validation and insert.
/// Only rows with a durable terminal-transfer receipt are guarded, so
/// preexisting non-transfer layout behavior remains unchanged.
pub(super) async fn ensure_layout_matches_transferred_terminals(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workspace_id: &str,
    proposed: &Value,
) -> Result<()> {
    let groups = proposed.get("groups").and_then(Value::as_object);
    let mut proposed_ids = BTreeSet::new();
    if let Some(groups) = groups {
        for group in groups.values() {
            let ids = group
                .get("tabIds")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("Layout contains an invalid pane tab list."))?;
            for id in ids {
                let id = id
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Layout contains an invalid pane tab id."))?;
                proposed_ids.insert(id.to_owned());
            }
        }
    }

    // Current ownership, not the last transfer's historical "to" field,
    // determines where a previously moved tab must occur. A future reverse
    // transfer uses the same receipt-bearing row and remains protected.
    let moved_rows = sqlx::query(
        "SELECT id, workspaceId, payloadJson FROM workspaceTabs \
         WHERE payloadJson LIKE '%lastTerminalTabTransfer%'",
    )
    .fetch_all(&mut **tx)
    .await?;
    for row in moved_rows {
        let id: String = row.try_get("id")?;
        let owner: String = row.try_get("workspaceId")?;
        let payload_json: String = row.try_get("payloadJson")?;
        let payload: Value = serde_json::from_str(&payload_json)?;
        if !payload
            .get("lastTerminalTabTransfer")
            .is_some_and(Value::is_object)
        {
            continue;
        }
        if owner == workspace_id {
            ensure!(
                proposed_ids.contains(&id),
                "Layout predates a Terminal transfer; refresh the destination workspace."
            );
        } else {
            ensure!(
                !proposed_ids.contains(&id),
                "Layout still contains a Terminal transferred out; refresh the source workspace."
            );
        }
    }
    Ok(())
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
