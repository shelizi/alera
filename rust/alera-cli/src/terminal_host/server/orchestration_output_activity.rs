//! Coalesce due PTY activity lookups without caching dispatch membership.
//! Ordinary terminals are usually not workers. Sharing a short collection
//! window keeps their reads and maintenance admissions from scaling one for
//! one with the number of terminals, while preserving the per-handle cadence.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alera_core::runtime::RuntimeStore;

use super::deferred_admission::{DeferredAdmission, DeferredRequestClass};
use super::ORCHESTRATION_ACTIVITY_WRITE_INTERVAL;

const COLLECTION_WINDOW: Duration = Duration::from_millis(20);
const REQUEST_TYPE: &str = "terminal.orchestrationActivity.persist";

#[derive(Default)]
struct PendingActivity {
    handles: HashSet<String>,
    scheduled: bool,
}

#[derive(Default)]
pub(super) struct OutputActivity {
    last_recorded: HashMap<String, Instant>,
    pending: Arc<Mutex<PendingActivity>>,
}

impl OutputActivity {
    pub(super) fn remove(&mut self, handle: &str) {
        self.last_recorded.remove(handle);
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .handles
            .remove(handle);
        // Keep the reservation until its worker takes the next batch,
        // even if this was the last pending handle.
    }

    pub(super) fn record(
        &mut self,
        handle: &str,
        store: &RuntimeStore,
        admission: &Arc<DeferredAdmission>,
    ) {
        if self
            .last_recorded
            .get(handle)
            .is_some_and(|last| last.elapsed() < ORCHESTRATION_ACTIVITY_WRITE_INTERVAL)
        {
            return;
        }
        self.last_recorded.insert(handle.to_owned(), Instant::now());
        {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            pending.handles.insert(handle.to_owned());
            if pending.scheduled {
                return;
            }
            pending.scheduled = true;
        }

        let pending = Arc::clone(&self.pending);
        let store = store.clone();
        let task = async move {
            tokio::time::sleep(COLLECTION_WINDOW).await;
            let handles: Vec<String> = {
                let mut pending = pending.lock().unwrap_or_else(|error| error.into_inner());
                pending.scheduled = false;
                std::mem::take(&mut pending.handles).into_iter().collect()
            };
            if handles.is_empty() {
                return;
            }
            match store
                .orchestration_output_activity_dispatch_ids(&handles)
                .await
            {
                Ok(ids) => {
                    for id in ids {
                        // Keep the existing transactional coordinator refresh
                        // and re-check dispatched status at the actual write.
                        if let Err(error) = store.record_orchestration_activity(&id).await {
                            tracing::debug!(dispatch_id = %id, "terminal activity write failed: {error}");
                        }
                    }
                }
                Err(error) => tracing::debug!("terminal activity lookup failed: {error}"),
            }
        };
        if let Err(error) =
            admission.schedule(DeferredRequestClass::Maintenance, REQUEST_TYPE, None, task)
        {
            // No worker owns this reservation. Preserve the existing
            // per-handle backoff when admission rejects maintenance work.
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            pending.scheduled = false;
            pending.handles.clear();
            tracing::debug!(
                session_id = %handle,
                "terminal orchestration activity persistence deferred: {}",
                error.wire_message()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_host::server::actor_test_harness::test_actor;
    use crate::terminal_host::server::orchestration_contract_tests::ready_task;

    async fn wait_until_complete(admission: &DeferredAdmission, expected: u64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = admission.snapshot();
                if snapshot["requestTypes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| {
                        entry["requestType"] == REQUEST_TYPE
                            && entry["completed"].as_u64().unwrap_or(0) >= expected
                    })
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("activity batch should finish");
    }

    fn request_metrics(admission: &DeferredAdmission) -> serde_json::Value {
        admission.snapshot()["requestTypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["requestType"] == REQUEST_TYPE)
            .unwrap()
            .clone()
    }

    #[tokio::test]
    async fn ordinary_terminal_burst_uses_one_maintenance_job_and_keeps_throttle() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 128, 1));
        let mut activity = OutputActivity::default();

        for i in 0..64 {
            let handle = format!("ordinary-{i}");
            for _ in 0..8 {
                activity.record(&handle, &store, &admission);
            }
        }
        assert_eq!(activity.pending.lock().unwrap().handles.len(), 64);
        assert_eq!(request_metrics(&admission)["queued"], 1);
        assert_eq!(request_metrics(&admission)["started"], 0);

        admission.add_test_permits(1);
        wait_until_complete(&admission, 1).await;
        assert!(!activity.pending.lock().unwrap().scheduled);
        assert!(activity.pending.lock().unwrap().handles.is_empty());
        for i in 0..64 {
            activity.record(&format!("ordinary-{i}"), &store, &admission);
        }
        assert_eq!(request_metrics(&admission)["started"], 1);
        assert_eq!(request_metrics(&admission)["queued"], 1);

        // A new dispatch/acceptance clears the existing handle throttle.
        activity.remove("ordinary-0");
        activity.record("ordinary-0", &store, &admission);
        let metrics = request_metrics(&admission);
        assert_eq!(
            metrics["started"].as_u64().unwrap() + metrics["pending"].as_u64().unwrap(),
            2
        );
        admission.add_test_permits(1);
        wait_until_complete(&admission, 2).await;
        assert_eq!(request_metrics(&admission)["started"], 2);
    }

    #[tokio::test]
    async fn mixed_activity_updates_only_accepted_dispatch_and_its_run() {
        let dir = tempfile::tempdir().unwrap();
        let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let store = &actor.runtime_store;
        let run = store
            .create_scoped_orchestration_coordinator_run("run", Some("coord"), 2000, "w1", 4)
            .await
            .unwrap();

        let mut dispatches = Vec::new();
        for handle in ["accepted", "waiting", "stalled", "completed"] {
            let task = ready_task(&actor, "coord").await;
            let dispatch = store
                .create_scoped_orchestration_dispatch(
                    &task,
                    handle,
                    Some(&run.id),
                    "w1",
                    "coord",
                    None,
                    "return-immediately",
                    "keep-open",
                )
                .await
                .unwrap();
            if handle != "waiting" {
                store
                    .accept_orchestration_dispatch(&dispatch.id, handle, "")
                    .await
                    .unwrap();
            }
            dispatches.push((handle, dispatch.id));
        }
        // Isolate the activity write contract from unrelated lifecycle logic.
        for (handle, id) in &dispatches {
            let status = match *handle {
                "accepted" => "dispatched",
                "waiting" => "awaiting_acceptance",
                other => other,
            };
            sqlx::query("UPDATE orchestrationDispatchContexts SET status = ?, last_activity_at = '2020-01-01 00:00:00' WHERE id = ?")
                .bind(status).bind(id).execute(store.pool()).await.unwrap();
        }
        sqlx::query("UPDATE orchestrationCoordinatorRuns SET last_activity_at = '2020-01-01 00:00:00' WHERE id = ?")
            .bind(&run.id).execute(store.pool()).await.unwrap();
        sqlx::query("CREATE TABLE activity_write_audit (dispatch_id TEXT NOT NULL)")
            .execute(store.pool())
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER audit_activity_update AFTER UPDATE OF last_activity_at ON orchestrationDispatchContexts BEGIN INSERT INTO activity_write_audit VALUES (NEW.id); END")
            .execute(store.pool()).await.unwrap();

        let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 128, 1));
        let mut activity = OutputActivity::default();
        for i in 0..64 {
            activity.record(&format!("shell-{i}"), store, &admission);
        }
        for (handle, _) in &dispatches {
            activity.record(handle, store, &admission);
        }
        assert_eq!(request_metrics(&admission)["queued"], 1);
        admission.add_test_permits(1);
        wait_until_complete(&admission, 1).await;

        let writes: Vec<String> =
            sqlx::query_scalar("SELECT dispatch_id FROM activity_write_audit")
                .fetch_all(store.pool())
                .await
                .unwrap();
        assert_eq!(writes, vec![dispatches[0].1.clone()]);
        let run = store
            .orchestration_coordinator_run_by_id(&run.id)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(run.last_activity_at, "2020-01-01 00:00:00");
        for (handle, id) in &dispatches {
            let current = store
                .orchestration_dispatch_by_id(id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                current.last_activity_at.as_deref() == Some("2020-01-01 00:00:00"),
                *handle != "accepted"
            );
        }

        // A previously ordinary terminal can join a new dispatch without
        // waiting for its original 30-second throttle.
        let task = ready_task(&actor, "coord").await;
        let dispatch = store
            .create_orchestration_dispatch(&task, "shell-0")
            .await
            .unwrap();
        activity.remove("shell-0");
        activity.record("shell-0", store, &admission);
        admission.add_test_permits(1);
        wait_until_complete(&admission, 2).await;
        let writes: Vec<String> =
            sqlx::query_scalar("SELECT dispatch_id FROM activity_write_audit ORDER BY rowid")
                .fetch_all(store.pool())
                .await
                .unwrap();
        assert_eq!(writes.last(), Some(&dispatch.id));
    }

    #[tokio::test]
    async fn batched_lookup_handles_boundaries_duplicates_and_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let store = &actor.runtime_store;
        let task = ready_task(&actor, "coord").await;
        let dispatch = store
            .create_orchestration_dispatch(&task, "active")
            .await
            .unwrap();
        let mut handles: Vec<String> = (0..300).map(|i| format!("shell-{i}")).collect();
        handles[127] = "active".into();
        handles[128] = "active".into();
        handles[299] = "active".into();
        assert!(store
            .orchestration_output_activity_dispatch_ids(&[])
            .await
            .unwrap()
            .is_empty());
        let ids = store
            .orchestration_output_activity_dispatch_ids(&handles)
            .await
            .unwrap();
        assert_eq!(ids, vec![dispatch.id]);
    }

    #[tokio::test]
    async fn removing_pending_handles_does_not_spawn_another_job() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 8, 1));
        let mut activity = OutputActivity::default();
        activity.record("old", &store, &admission);
        activity.remove("old");
        assert!(activity.pending.lock().unwrap().handles.is_empty());
        activity.record("new", &store, &admission);
        assert_eq!(request_metrics(&admission)["queued"], 1);
        admission.add_test_permits(1);
        wait_until_complete(&admission, 1).await;
    }
}
