use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::Semaphore;

pub use super::history_store::TerminalHostCheckpoint;
use super::history_store::TerminalHostHistoryStore;

const HISTORY_FLUSH_CONCURRENCY: usize = 2;
const HISTORY_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct TerminalHostHistoryRepository {
    inner: Arc<HistoryRepositoryInner>,
}

struct HistoryRepositoryInner {
    store: TerminalHostHistoryStore,
    sessions: Mutex<HashMap<String, SessionPersistenceState>>,
    flush_slots: Semaphore,
}

#[derive(Default)]
struct SessionPersistenceState {
    checkpoint: Option<TerminalHostCheckpoint>,
    checkpoint_revision: Option<u64>,
    outputs: BTreeMap<i64, PendingOutput>,
    memory_tail: Vec<u8>,
    max_bytes: usize,
    delete_revision: Option<u64>,
    trim_revision: Option<u64>,
    next_revision: u64,
    worker_running: bool,
    last_error: Option<String>,
}

#[derive(Clone)]
struct PendingOutput {
    revision: u64,
    data: Vec<u8>,
}

#[derive(Clone)]
enum FlushSnapshot {
    Delete {
        revision: u64,
    },
    Persist {
        checkpoint: Option<(u64, TerminalHostCheckpoint)>,
        outputs: Vec<(i64, u64, Vec<u8>)>,
        trim_revision: Option<u64>,
        max_bytes: usize,
    },
}

impl SessionPersistenceState {
    fn next_revision(&mut self) -> u64 {
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.next_revision
    }

    fn snapshot(&self) -> Option<FlushSnapshot> {
        if let Some(revision) = self.delete_revision {
            return Some(FlushSnapshot::Delete { revision });
        }
        if self.checkpoint_revision.is_none()
            && self.outputs.is_empty()
            && self.trim_revision.is_none()
        {
            return None;
        }
        Some(FlushSnapshot::Persist {
            checkpoint: self.checkpoint_revision.and_then(|revision| {
                self.checkpoint
                    .as_ref()
                    .cloned()
                    .map(|checkpoint| (revision, checkpoint))
            }),
            outputs: self
                .outputs
                .iter()
                .map(|(sequence, output)| (*sequence, output.revision, output.data.clone()))
                .collect(),
            trim_revision: self.trim_revision,
            max_bytes: self.max_bytes,
        })
    }

    fn trim_pending_outputs(&mut self) {
        let mut total = self
            .outputs
            .values()
            .map(|output| output.data.len())
            .sum::<usize>();
        while total > self.max_bytes {
            let Some(sequence) = self.outputs.keys().next().copied() else {
                break;
            };
            let excess = total - self.max_bytes;
            let len = self
                .outputs
                .get(&sequence)
                .map_or(0, |output| output.data.len());
            if len <= excess {
                self.outputs.remove(&sequence);
                total = total.saturating_sub(len);
                continue;
            }

            let revision = self.next_revision();
            if let Some(output) = self.outputs.get_mut(&sequence) {
                output.data.drain(..excess);
                output.revision = revision;
                total -= excess;
            }
        }
    }
}

impl TerminalHostHistoryRepository {
    pub async fn open(runtime_dir: &std::path::Path) -> Result<Self> {
        let store = TerminalHostHistoryStore::open(runtime_dir).await?;
        Ok(Self::from_store(store))
    }

    fn from_store(store: TerminalHostHistoryStore) -> Self {
        Self {
            inner: Arc::new(HistoryRepositoryInner {
                store,
                sessions: Mutex::new(HashMap::new()),
                flush_slots: Semaphore::new(HISTORY_FLUSH_CONCURRENCY),
            }),
        }
    }

    pub fn queue_checkpoint(&self, mut checkpoint: TerminalHostCheckpoint, max_bytes: usize) {
        retain_tail(&mut checkpoint.buffer, max_bytes);
        let session_id = checkpoint.session_id.clone();
        let memory_tail = std::mem::take(&mut checkpoint.buffer);
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            state.max_bytes = max_bytes;
            state.delete_revision = None;
            state.memory_tail = memory_tail;
            let checkpoint_revision = state.next_revision();
            state.checkpoint = Some(checkpoint);
            state.checkpoint_revision = Some(checkpoint_revision);
            state.trim_pending_outputs();
            let trim_revision = state.next_revision();
            state.trim_revision = Some(trim_revision);
        }
        self.ensure_worker(session_id);
    }

    pub fn queue_output(&self, session_id: String, sequence: i64, data: Vec<u8>, max_bytes: usize) {
        if data.is_empty() {
            return;
        }
        let mut queued = false;
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            if state.delete_revision.is_none() {
                state.max_bytes = max_bytes;
                append_tail(&mut state.memory_tail, &data, max_bytes);
                let revision = state.next_revision();
                state
                    .outputs
                    .insert(sequence, PendingOutput { revision, data });
                state.trim_pending_outputs();
                queued = true;
            }
        }
        if queued {
            self.ensure_worker(session_id);
        }
    }

    pub fn queue_trim(&self, session_id: impl Into<String>, max_bytes: usize) {
        let session_id = session_id.into();
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            if state.delete_revision.is_some() {
                return;
            }
            state.max_bytes = max_bytes;
            retain_tail(&mut state.memory_tail, max_bytes);
            if let Some(checkpoint) = state.checkpoint.as_mut() {
                retain_tail(&mut checkpoint.buffer, max_bytes);
            }
            state.trim_pending_outputs();
            let revision = state.next_revision();
            state.trim_revision = Some(revision);
        }
        self.ensure_worker(session_id);
    }

    pub fn queue_delete(&self, session_id: impl Into<String>) {
        let session_id = session_id.into();
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            let revision = state.next_revision();
            state.delete_revision = Some(revision);
            state.checkpoint = None;
            state.checkpoint_revision = None;
            state.outputs.clear();
            state.trim_revision = None;
            state.memory_tail.clear();
            state.last_error = None;
        }
        self.ensure_worker(session_id);
    }

    pub async fn read(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Result<Option<TerminalHostCheckpoint>> {
        if let Some(cached) = self.cached_checkpoint(session_id) {
            return Ok(cached);
        }
        if self.is_deleted(session_id) {
            return Ok(None);
        }

        let persisted = self.inner.store.read(session_id, max_bytes).await?;
        let Some(mut persisted) = persisted else {
            return Ok(None);
        };
        retain_tail(&mut persisted.buffer, max_bytes);

        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.to_string()).or_default();
            if state.delete_revision.is_some() {
                return Ok(None);
            }
            if let Some(checkpoint) = state.checkpoint.as_ref() {
                let mut checkpoint = checkpoint.clone();
                checkpoint.buffer = state.memory_tail.clone();
                return Ok(Some(checkpoint));
            }
            state.max_bytes = max_bytes;
            state.memory_tail = persisted.buffer.clone();
            let mut cached = persisted.clone();
            cached.buffer.clear();
            state.checkpoint = Some(cached);
        }

        // Cold reads return only the bounded tail. Physical compaction is
        // maintenance and must not delay restore/liveness-sensitive requests.
        self.queue_trim(session_id.to_string(), max_bytes);
        Ok(Some(persisted))
    }

    pub async fn next_output_sequence(&self, session_id: &str) -> Result<i64> {
        let pending_next = {
            let sessions = self.lock_sessions();
            sessions
                .get(session_id)
                .and_then(|state| state.outputs.keys().next_back().copied())
                .map(|sequence| sequence.saturating_add(1))
                .unwrap_or(0)
        };
        Ok(self
            .inner
            .store
            .next_output_sequence(session_id)
            .await?
            .max(pending_next))
    }

    fn cached_checkpoint(&self, session_id: &str) -> Option<Option<TerminalHostCheckpoint>> {
        let sessions = self.lock_sessions();
        let state = sessions.get(session_id)?;
        if state.delete_revision.is_some() {
            return Some(None);
        }
        let checkpoint = state.checkpoint.as_ref()?.clone();
        let mut checkpoint = checkpoint;
        checkpoint.buffer = state.memory_tail.clone();
        Some(Some(checkpoint))
    }

    fn is_deleted(&self, session_id: &str) -> bool {
        self.lock_sessions()
            .get(session_id)
            .is_some_and(|state| state.delete_revision.is_some())
    }

    fn ensure_worker(&self, session_id: String) {
        let should_spawn = {
            let mut sessions = self.lock_sessions();
            let Some(state) = sessions.get_mut(&session_id) else {
                return;
            };
            if state.worker_running {
                false
            } else {
                state.worker_running = true;
                true
            }
        };
        if !should_spawn {
            return;
        }

        let repository = self.clone();
        tokio::spawn(async move {
            repository.flush_worker(session_id).await;
        });
    }

    async fn flush_worker(&self, session_id: String) {
        loop {
            let Some(snapshot) = self.next_snapshot_or_stop(&session_id) else {
                return;
            };
            let permit = match self.inner.flush_slots.acquire().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let result = self.persist_snapshot(&session_id, &snapshot).await;
            drop(permit);

            match result {
                Ok(()) => {
                    self.ack_snapshot(&session_id, &snapshot);
                }
                Err(error) => {
                    let message = error.to_string();
                    {
                        let mut sessions = self.lock_sessions();
                        if let Some(state) = sessions.get_mut(&session_id) {
                            state.last_error = Some(message.clone());
                        }
                    }
                    tracing::warn!(
                        session_id = %session_id,
                        "terminal history persistence failed; keeping data in memory for retry: {message}"
                    );
                    tokio::time::sleep(HISTORY_RETRY_DELAY).await;
                }
            }
        }
    }

    pub async fn flush_pending(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                let idle = {
                    let sessions = self.lock_sessions();
                    sessions.values().all(|state| {
                        !state.worker_running
                            && state.checkpoint_revision.is_none()
                            && state.outputs.is_empty()
                            && state.trim_revision.is_none()
                            && state.delete_revision.is_none()
                    })
                };
                if idle {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    fn next_snapshot_or_stop(&self, session_id: &str) -> Option<FlushSnapshot> {
        let mut sessions = self.lock_sessions();
        let snapshot = sessions.get(session_id)?.snapshot();
        if snapshot.is_none() {
            // The repository is a write-behind buffer, not a second durable
            // cache. Once SQLite has acknowledged everything, release the
            // session state so memory is reclaimed automatically.
            sessions.remove(session_id);
        }
        snapshot
    }

    async fn persist_snapshot(&self, session_id: &str, snapshot: &FlushSnapshot) -> Result<()> {
        match snapshot {
            FlushSnapshot::Delete { .. } => self.inner.store.delete(session_id).await,
            FlushSnapshot::Persist {
                checkpoint,
                outputs,
                trim_revision,
                max_bytes,
            } => {
                let output_rows = outputs
                    .iter()
                    .map(|(sequence, _, data)| (*sequence, data.clone()))
                    .collect::<Vec<_>>();
                self.inner
                    .store
                    .persist_session(
                        session_id,
                        checkpoint.as_ref().map(|(_, checkpoint)| checkpoint),
                        &output_rows,
                    )
                    .await?;
                // Output batches are frequent (currently ~100 ms). Persist them
                // without running the expensive window-function trim each time;
                // checkpoint/configuration events request trim at the slower
                // maintenance cadence instead.
                if trim_revision.is_some() {
                    self.inner
                        .store
                        .trim_session(session_id, *max_bytes)
                        .await?;
                }
                Ok(())
            }
        }
    }

    fn ack_snapshot(&self, session_id: &str, snapshot: &FlushSnapshot) {
        let mut sessions = self.lock_sessions();
        let Some(state) = sessions.get_mut(session_id) else {
            return;
        };
        match snapshot {
            FlushSnapshot::Delete { revision } => {
                if state.delete_revision == Some(*revision) {
                    sessions.remove(session_id);
                    return;
                }
            }
            FlushSnapshot::Persist {
                checkpoint,
                outputs,
                trim_revision,
                ..
            } => {
                if let Some((revision, _)) = checkpoint {
                    if state.checkpoint_revision == Some(*revision) {
                        state.checkpoint_revision = None;
                    }
                }
                for (sequence, revision, _) in outputs {
                    if state
                        .outputs
                        .get(sequence)
                        .is_some_and(|output| output.revision == *revision)
                    {
                        state.outputs.remove(sequence);
                    }
                }
                if let Some(revision) = trim_revision {
                    if state.trim_revision == Some(*revision) {
                        state.trim_revision = None;
                    }
                }
                state.last_error = None;
            }
        }
    }

    fn lock_sessions(&self) -> MutexGuard<'_, HashMap<String, SessionPersistenceState>> {
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    #[cfg(test)]
    async fn wait_until_idle(&self, session_id: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let idle = {
                    let sessions = self.lock_sessions();
                    sessions.get(session_id).is_none_or(|state| {
                        !state.worker_running
                            && state.checkpoint_revision.is_none()
                            && state.outputs.is_empty()
                            && state.trim_revision.is_none()
                            && state.delete_revision.is_none()
                    })
                };
                if idle {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("history repository should become idle");
    }
}

fn retain_tail(buffer: &mut Vec<u8>, max_bytes: usize) {
    if buffer.len() <= max_bytes {
        return;
    }
    let start = buffer.len() - max_bytes;
    buffer.drain(..start);
}

fn append_tail(buffer: &mut Vec<u8>, data: &[u8], max_bytes: usize) {
    if max_bytes == 0 {
        buffer.clear();
        return;
    }
    if data.len() >= max_bytes {
        buffer.clear();
        buffer.extend_from_slice(&data[data.len() - max_bytes..]);
        return;
    }
    let required = buffer.len().saturating_add(data.len());
    if required > max_bytes {
        buffer.drain(..required - max_bytes);
    }
    buffer.extend_from_slice(data);
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn checkpoint(session_id: &str, buffer: &[u8]) -> TerminalHostCheckpoint {
        TerminalHostCheckpoint {
            session_id: session_id.to_string(),
            workspace_id: "ws".to_string(),
            tab_id: "tab".to_string(),
            working_directory: "/tmp/project".to_string(),
            running: true,
            exit_code: None,
            ended_at: None,
            output_stream_bytes: buffer.len() as u64,
            updated_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            buffer: buffer.to_vec(),
        }
    }

    #[tokio::test]
    async fn memory_cache_is_authoritative_before_sqlite_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b"abc"), 1024);
        repository.queue_output("s1".to_string(), 0, b"def".to_vec(), 1024);

        let read = repository.read("s1", 1024).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"abcdef");
    }

    #[tokio::test]
    async fn memory_cache_keeps_only_the_configured_tail() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b"abcd"), 5);
        repository.queue_output("s1".to_string(), 0, b"efgh".to_vec(), 5);

        let read = repository.read("s1", 5).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"defgh");
    }

    #[tokio::test]
    async fn background_flush_persists_buffered_output() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b""), 1024);
        repository.queue_output("s1".to_string(), 0, b"hello".to_vec(), 1024);
        repository.wait_until_idle("s1").await;

        let persisted = repository
            .inner
            .store
            .read("s1", 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.buffer, b"hello");
    }

    #[tokio::test]
    async fn flush_pending_waits_for_durable_ack_and_releases_memory() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b""), 1024);
        repository.queue_output("s1".to_string(), 0, b"hello".to_vec(), 1024);

        assert!(repository.flush_pending(Duration::from_secs(2)).await);
        assert!(!repository.lock_sessions().contains_key("s1"));
        let persisted = repository
            .inner
            .store
            .read("s1", 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.buffer, b"hello");
    }

    #[tokio::test]
    async fn cold_read_schedules_physical_trim_without_blocking_the_read_contract() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
            store.upsert(checkpoint("s1", b"")).await.unwrap();
            store.append_output("s1", 0, b"abc").await.unwrap();
            store.append_output("s1", 1, b"de").await.unwrap();
            store.append_output("s1", 2, b"fg").await.unwrap();
        }
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        let read = repository.read("s1", 5).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"cdefg");

        repository.wait_until_idle("s1").await;
        let persisted = repository
            .inner
            .store
            .read("s1", 100)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.buffer, b"cdefg");
    }

    #[tokio::test]
    async fn sqlite_failure_keeps_dirty_history_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.inner.store.close_for_test().await;

        repository.queue_checkpoint(checkpoint("s1", b"abc"), 1024);
        repository.queue_output("s1".to_string(), 0, b"def".to_vec(), 1024);

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let failed = repository
                    .lock_sessions()
                    .get("s1")
                    .is_some_and(|state| state.last_error.is_some());
                if failed {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("closed SQLite pool should fail the background flush");

        let read = repository.read("s1", 1024).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"abcdef");
        let sessions = repository.lock_sessions();
        let state = sessions.get("s1").unwrap();
        assert!(state.checkpoint_revision.is_some());
        assert!(!state.outputs.is_empty());
        assert!(state.worker_running);
    }

    #[tokio::test]
    async fn delete_hides_cached_history_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.queue_checkpoint(checkpoint("s1", b"hello"), 1024);

        repository.queue_delete("s1");

        assert!(repository.read("s1", 1024).await.unwrap().is_none());
    }

    #[test]
    fn pending_output_cap_keeps_latest_bytes_without_acknowledging_old_snapshot() {
        let mut state = SessionPersistenceState {
            max_bytes: 5,
            ..Default::default()
        };
        let first_revision = state.next_revision();
        state.outputs.insert(
            0,
            PendingOutput {
                revision: first_revision,
                data: b"abcd".to_vec(),
            },
        );
        let second_revision = state.next_revision();
        state.outputs.insert(
            1,
            PendingOutput {
                revision: second_revision,
                data: b"efgh".to_vec(),
            },
        );

        state.trim_pending_outputs();

        assert_eq!(state.outputs[&0].data, b"d");
        assert_ne!(state.outputs[&0].revision, first_revision);
        assert_eq!(state.outputs[&1].data, b"efgh");
    }
}
