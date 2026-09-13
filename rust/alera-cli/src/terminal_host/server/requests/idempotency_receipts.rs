use alera_core::runtime::RuntimeStore;
use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::terminal_host::host_error::{HostError, HostResult};

const RECEIPT_RETENTION_MILLIS: i64 = 7 * 24 * 60 * 60 * 1_000;
const RECEIPT_CAPACITY_PER_OPERATION_SCOPE: i64 = 256;
const RECEIPT_GLOBAL_CAPACITY: i64 = 4_096;
const CLIENT_MUTATION_ID_MAX_BYTES: usize = 128;

const RECEIPT_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS terminalHostIdempotencyReceipts (
    operation TEXT NOT NULL,
    callerScope TEXT NOT NULL,
    clientMutationId TEXT NOT NULL,
    payloadDigest TEXT NOT NULL,
    state TEXT NOT NULL,
    resultJson TEXT,
    createdAt INTEGER NOT NULL,
    PRIMARY KEY (operation, callerScope, clientMutationId)
);";

#[derive(Debug, PartialEq)]
pub(in crate::terminal_host::server) enum ReceiptPrepareOutcome {
    Created,
    Replay(Value),
    Conflict,
    InProgress,
}

pub(in crate::terminal_host::server) fn optional_client_mutation_id(
    payload: &Value,
) -> HostResult<Option<String>> {
    match payload.get("clientMutationId") {
        None => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => {
            let value = value.trim().to_string();
            if value.len() > CLIENT_MUTATION_ID_MAX_BYTES {
                return Err(HostError::format(format!(
                    "clientMutationId must not exceed {CLIENT_MUTATION_ID_MAX_BYTES} bytes."
                )));
            }
            Ok(Some(value))
        }
        Some(_) => Err(HostError::format(
            "clientMutationId must be a non-empty string.",
        )),
    }
}

pub(in crate::terminal_host::server) fn payload_digest(payload: &Value) -> Result<String> {
    let mut payload = payload.clone();
    if let Some(object) = payload.as_object_mut() {
        object.remove("clientMutationId");
    }
    let encoded = serde_json::to_vec(&payload).context("Could not encode idempotency payload")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

pub(in crate::terminal_host::server) async fn prepare_receipt(
    store: &RuntimeStore,
    operation: &str,
    caller_scope: &str,
    client_mutation_id: &str,
    payload_digest: &str,
    settled_result: Option<&Value>,
) -> Result<ReceiptPrepareOutcome> {
    sqlx::query(RECEIPT_SCHEMA)
        .execute(store.pool())
        .await
        .context("Could not initialize terminal idempotency receipt storage")?;

    let mut connection = store
        .pool()
        .acquire()
        .await
        .context("Could not open terminal idempotency receipt storage")?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .context("Could not lock terminal idempotency receipt storage")?;

    let result: Result<ReceiptPrepareOutcome> = async {
        let cutoff = now_millis() - RECEIPT_RETENTION_MILLIS;
        sqlx::query(
            "DELETE FROM terminalHostIdempotencyReceipts WHERE createdAt < ?",
        )
        .bind(cutoff)
        .execute(&mut *connection)
        .await?;

        let existing = sqlx::query(
            "SELECT payloadDigest, state, resultJson
             FROM terminalHostIdempotencyReceipts
             WHERE operation = ? AND callerScope = ? AND clientMutationId = ?",
        )
        .bind(operation)
        .bind(caller_scope)
        .bind(client_mutation_id)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(row) = existing {
            let existing_digest: String = row.try_get("payloadDigest")?;
            if existing_digest != payload_digest {
                return Ok(ReceiptPrepareOutcome::Conflict);
            }
            let state: String = row.try_get("state")?;
            return match state.as_str() {
                "settled" => {
                    let result_json: String = row
                        .try_get::<Option<String>, _>("resultJson")?
                        .ok_or_else(|| anyhow::anyhow!("Settled receipt is missing its result"))?;
                    Ok(ReceiptPrepareOutcome::Replay(serde_json::from_str(
                        &result_json,
                    )?))
                }
                "pending" => Ok(ReceiptPrepareOutcome::InProgress),
                other => Err(anyhow::anyhow!(
                    "Unknown terminal idempotency receipt state: {other}"
                )),
            };
        }

        let (state, result_json) = match settled_result {
            Some(result) => ("settled", Some(serde_json::to_string(result)?)),
            None => ("pending", None),
        };
        sqlx::query(
            "INSERT INTO terminalHostIdempotencyReceipts
                (operation, callerScope, clientMutationId, payloadDigest, state, resultJson, createdAt)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(operation)
        .bind(caller_scope)
        .bind(client_mutation_id)
        .bind(payload_digest)
        .bind(state)
        .bind(result_json)
        .bind(now_millis())
        .execute(&mut *connection)
        .await?;

        sqlx::query(
            "DELETE FROM terminalHostIdempotencyReceipts WHERE rowid IN (
               SELECT rowid FROM terminalHostIdempotencyReceipts
               WHERE operation = ? AND callerScope = ?
               ORDER BY createdAt DESC, rowid DESC LIMIT -1 OFFSET ?
             )",
        )
        .bind(operation)
        .bind(caller_scope)
        .bind(RECEIPT_CAPACITY_PER_OPERATION_SCOPE)
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "DELETE FROM terminalHostIdempotencyReceipts WHERE rowid IN (
               SELECT rowid FROM terminalHostIdempotencyReceipts
               ORDER BY createdAt DESC, rowid DESC LIMIT -1 OFFSET ?
             )",
        )
        .bind(RECEIPT_GLOBAL_CAPACITY)
        .execute(&mut *connection)
        .await?;
        Ok(ReceiptPrepareOutcome::Created)
    }
    .await;

    match result {
        Ok(outcome) => {
            sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .context("Could not commit terminal idempotency receipt")?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            Err(error).context("Could not persist terminal idempotency receipt")
        }
    }
}

pub(in crate::terminal_host::server) async fn settle_receipt(
    store: &RuntimeStore,
    operation: &str,
    caller_scope: &str,
    client_mutation_id: &str,
    payload_digest: &str,
    result: &Value,
) -> Result<()> {
    let result_json = serde_json::to_string(result)?;
    let updated = sqlx::query(
        "UPDATE terminalHostIdempotencyReceipts
         SET state = 'settled', resultJson = ?
         WHERE operation = ? AND callerScope = ? AND clientMutationId = ?
           AND payloadDigest = ? AND state = 'pending'",
    )
    .bind(&result_json)
    .bind(operation)
    .bind(caller_scope)
    .bind(client_mutation_id)
    .bind(payload_digest)
    .execute(store.pool())
    .await
    .context("Could not settle terminal idempotency receipt")?;
    if updated.rows_affected() == 1 {
        return Ok(());
    }

    let existing = sqlx::query(
        "SELECT payloadDigest, state, resultJson
         FROM terminalHostIdempotencyReceipts
         WHERE operation = ? AND callerScope = ? AND clientMutationId = ?",
    )
    .bind(operation)
    .bind(caller_scope)
    .bind(client_mutation_id)
    .fetch_optional(store.pool())
    .await
    .context("Could not verify terminal idempotency receipt")?;
    let Some(existing) = existing else {
        return Err(anyhow::anyhow!("Terminal idempotency receipt disappeared"));
    };
    let existing_digest: String = existing.try_get("payloadDigest")?;
    let state: String = existing.try_get("state")?;
    if existing_digest == payload_digest && state == "settled" {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "Terminal idempotency receipt changed before it could be settled"
    ))
}

pub(in crate::terminal_host::server) async fn remove_receipt(
    store: &RuntimeStore,
    operation: &str,
    caller_scope: &str,
    client_mutation_id: &str,
    payload_digest: &str,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM terminalHostIdempotencyReceipts
         WHERE operation = ? AND callerScope = ? AND clientMutationId = ?
           AND payloadDigest = ?",
    )
    .bind(operation)
    .bind(caller_scope)
    .bind(client_mutation_id)
    .bind(payload_digest)
    .execute(store.pool())
    .await
    .context("Could not remove terminal idempotency receipt")?;
    Ok(())
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
